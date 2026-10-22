#include "uvmcheck/rules/Rule.h"
#include "uvmcheck/rules/RuleRegistry.h"
#include "slang/syntax/AllSyntax.h"
#include "slang/syntax/SyntaxTree.h"
#include "slang/syntax/SyntaxVisitor.h"
#include <unordered_map>
#include <unordered_set>

namespace uvmcheck::rules::deadcode {

class ClassPropertyReadWriteVisitor : public slang::syntax::SyntaxVisitor<ClassPropertyReadWriteVisitor> {
public:
    std::unordered_set<std::string> writtenVars;
    std::unordered_set<std::string> readVars;

    void handle(const slang::syntax::BinaryExpressionSyntax& expr) {
        if (expr.kind == slang::syntax::SyntaxKind::AssignmentExpression) {
            std::string lhs = std::string(expr.left->getLastToken().valueText());
            writtenVars.insert(lhs);
            expr.right->visit(*this);
            return;
        }
        if (expr.kind == slang::syntax::SyntaxKind::AddAssignmentExpression ||
            expr.kind == slang::syntax::SyntaxKind::SubtractAssignmentExpression ||
            expr.kind == slang::syntax::SyntaxKind::MultiplyAssignmentExpression ||
            expr.kind == slang::syntax::SyntaxKind::DivideAssignmentExpression) {
            std::string lhs = std::string(expr.left->getLastToken().valueText());
            writtenVars.insert(lhs);
            readVars.insert(lhs);
            expr.right->visit(*this);
            return;
        }
        visitDefault(expr);
    }

    void handle(const slang::syntax::PostfixUnaryExpressionSyntax& expr) {
        std::string name = std::string(expr.operand->getLastToken().valueText());
        writtenVars.insert(name);
        readVars.insert(name);
        visitDefault(expr);
    }

    void handle(const slang::syntax::PrefixUnaryExpressionSyntax& expr) {
        std::string name = std::string(expr.operand->getLastToken().valueText());
        writtenVars.insert(name);
        readVars.insert(name);
        visitDefault(expr);
    }

    void handle(const slang::syntax::IdentifierNameSyntax& id) {
        readVars.insert(std::string(id.identifier.valueText()));
        visitDefault(id);
    }
};

class UnwrittenPropertyVisitor : public engine::SyntaxVisitor<UnwrittenPropertyVisitor> {
public:
    explicit UnwrittenPropertyVisitor(core::DiagnosticEngine& diags) : diags(diags) {}

    void handle(const slang::syntax::ClassDeclarationSyntax& cls) {
        std::unordered_map<std::string, slang::SourceLocation> uninitProps;
        std::string className = std::string(cls.name.valueText());

        // Collect non-rand class properties that lack an inline initializer
        for (const auto* member : cls.items) {
            const slang::syntax::DataDeclarationSyntax* dataDecl = nullptr;
            bool isRand = false;

            if (member->kind == slang::syntax::SyntaxKind::DataDeclaration) {
                dataDecl = &member->as<slang::syntax::DataDeclarationSyntax>();
            } else if (member->kind == slang::syntax::SyntaxKind::ClassPropertyDeclaration) {
                const auto& prop = member->as<slang::syntax::ClassPropertyDeclarationSyntax>();
                for (const auto& q : prop.qualifiers) {
                    if (q.kind == slang::parsing::TokenKind::RandKeyword ||
                        q.kind == slang::parsing::TokenKind::RandCKeyword) {
                        isRand = true;
                        break;
                    }
                }
                if (prop.declaration->kind == slang::syntax::SyntaxKind::DataDeclaration) {
                    dataDecl = &prop.declaration->as<slang::syntax::DataDeclarationSyntax>();
                }
            }

            if (dataDecl && !isRand) {
                for (const auto* decl : dataDecl->declarators) {
                    if (decl->initializer == nullptr) {
                        uninitProps[std::string(decl->name.valueText())] = decl->name.location();
                    }
                }
            }
        }

        // Analyze usage across all methods in class
        ClassPropertyReadWriteVisitor usage;
        for (const auto* member : cls.items) {
            if (member->kind == slang::syntax::SyntaxKind::FunctionDeclaration ||
                member->kind == slang::syntax::SyntaxKind::TaskDeclaration ||
                member->kind == slang::syntax::SyntaxKind::ClassMethodDeclaration) {
                member->visit(usage);
            }
        }

        // Flag properties that are read but never initialized or written
        for (const auto& [propName, loc] : uninitProps) {
            if (usage.readVars.count(propName) > 0 && usage.writtenVars.count(propName) == 0) {
                diags.addViolation(
                    "SV-DED-006",
                    "UnwrittenClassProperty",
                    "Class property '" + propName + "' is read in class '" + className +
                    "' without an inline default initializer and is never assigned or initialized in any method. "
                    "It will consistently evaluate to its uninitialized default value (0 or 'x), risking dead execution paths. "
                    "Initialize the property at declaration or assign it inside the constructor 'new()'.",
                    core::Severity::Warning,
                    loc
                );
            }
        }

        visitDefault(cls);
    }

private:
    core::DiagnosticEngine& diags;
};

class UnwrittenClassProperty : public Rule {
public:
    std::string_view getId() const override { return "SV-DED-006"; }
    std::string_view getName() const override { return "UnwrittenClassProperty"; }
    std::string_view getDescription() const override {
        return "Flags non-rand class properties that lack an initializer, are read in methods, but never written or assigned. "
               "Unwritten properties always evaluate to uninitialized defaults, risking dead branches and silent logic bugs.";
    }
    std::string_view getCategory() const override { return "DeadCode"; }
    core::Severity getDefaultSeverity() const override { return core::Severity::Warning; }

    engine::RuleTask check(engine::RuleContext& ctx) override {
        auto& diags = ctx.diags();
        UnwrittenPropertyVisitor visitor(diags);
        co_await ctx.visitSyntax(visitor);
    }
};

} // namespace uvmcheck::rules::deadcode

namespace uvmcheck::rules {
std::unique_ptr<Rule> createUnwrittenClassProperty() {
    return std::make_unique<deadcode::UnwrittenClassProperty>();
}
}
