#include "uvmcheck/rules/Rule.h"
#include "uvmcheck/rules/RuleRegistry.h"
#include "slang/syntax/AllSyntax.h"
#include "slang/syntax/SyntaxTree.h"
#include "slang/syntax/SyntaxVisitor.h"
#include <unordered_map>
#include <unordered_set>

namespace uvmcheck::rules::deadcode {

class ClassPropertyUsageVisitor : public slang::syntax::SyntaxVisitor<ClassPropertyUsageVisitor> {
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
    }

    void handle(const slang::syntax::PrefixUnaryExpressionSyntax& expr) {
        std::string name = std::string(expr.operand->getLastToken().valueText());
        writtenVars.insert(name);
    }

    void handle(const slang::syntax::IdentifierNameSyntax& id) {
        readVars.insert(std::string(id.identifier.valueText()));
        visitDefault(id);
    }
};

class UnreadPropertyVisitor : public engine::SyntaxVisitor<UnreadPropertyVisitor> {
public:
    explicit UnreadPropertyVisitor(core::DiagnosticEngine& diags) : diags(diags) {}

    void handle(const slang::syntax::ClassDeclarationSyntax& cls) {
        std::unordered_map<std::string, slang::SourceLocation> candidateProps;
        std::string className = std::string(cls.name.valueText());

        // Collect class properties
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
                    candidateProps[std::string(decl->name.valueText())] = decl->name.location();
                }
            }
        }

        // Analyze usage across all methods and statements in class
        ClassPropertyUsageVisitor usage;
        for (const auto* member : cls.items) {
            if (member->kind == slang::syntax::SyntaxKind::FunctionDeclaration ||
                member->kind == slang::syntax::SyntaxKind::TaskDeclaration ||
                member->kind == slang::syntax::SyntaxKind::ClassMethodDeclaration) {
                member->visit(usage);
            }
        }

        // Flag properties that are written but never read
        for (const auto& [propName, loc] : candidateProps) {
            if (usage.writtenVars.count(propName) > 0 && usage.readVars.count(propName) == 0) {
                diags.addViolation(
                    "SV-DED-005",
                    "UnreadClassProperty",
                    "Class property '" + propName + "' is assigned or updated, but its value is never read in class '" +
                    className + "'. Unread properties waste heap memory and indicate dead instrumentation or redundant state.",
                    core::Severity::Info,
                    loc
                );
            }
        }

        visitDefault(cls);
    }

private:
    core::DiagnosticEngine& diags;
};

class UnreadClassProperty : public Rule {
public:
    std::string_view getId() const override { return "SV-DED-005"; }
    std::string_view getName() const override { return "UnreadClassProperty"; }
    std::string_view getDescription() const override {
        return "Flags class properties that are assigned or written to, but never read anywhere in the class. "
               "Unread properties indicate dead instrumentation or abandoned design state.";
    }
    std::string_view getCategory() const override { return "DeadCode"; }
    core::Severity getDefaultSeverity() const override { return core::Severity::Info; }

    engine::RuleTask check(engine::RuleContext& ctx) override {
        auto& diags = ctx.diags();
        UnreadPropertyVisitor visitor(diags);
        co_await ctx.visitSyntax(visitor);
    }
};

} // namespace uvmcheck::rules::deadcode

namespace uvmcheck::rules {
std::unique_ptr<Rule> createUnreadClassProperty() {
    return std::make_unique<deadcode::UnreadClassProperty>();
}
}
