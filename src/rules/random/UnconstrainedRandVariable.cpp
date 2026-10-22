#include "uvmcheck/rules/Rule.h"
#include "uvmcheck/rules/RuleRegistry.h"
#include "slang/syntax/AllSyntax.h"
#include "slang/syntax/SyntaxTree.h"
#include "slang/syntax/SyntaxVisitor.h"
#include <unordered_map>
#include <unordered_set>
#include <string>

namespace uvmcheck::rules::random {

class ConstraintIdentifierCollector : public slang::syntax::SyntaxVisitor<ConstraintIdentifierCollector> {
public:
    std::unordered_set<std::string> identifiers;

    void handle(const slang::syntax::IdentifierNameSyntax& id) {
        identifiers.insert(std::string(id.identifier.valueText()));
        visitDefault(id);
    }
};

class UnconstrainedRandVisitor : public engine::SyntaxVisitor<UnconstrainedRandVisitor> {
public:
    explicit UnconstrainedRandVisitor(core::DiagnosticEngine& diags) : diags(diags) {}

    void handle(const slang::syntax::ClassDeclarationSyntax& cls) {
        std::unordered_map<std::string, slang::SourceLocation> randProps;
        std::string className = std::string(cls.name.valueText());

        // 1. Collect rand properties
        for (const auto* member : cls.items) {
            if (member->kind == slang::syntax::SyntaxKind::ClassPropertyDeclaration) {
                const auto& prop = member->as<slang::syntax::ClassPropertyDeclarationSyntax>();
                bool isRand = false;
                bool isStatic = false;

                for (const auto& q : prop.qualifiers) {
                    if (q.kind == slang::parsing::TokenKind::RandKeyword) {
                        isRand = true;
                    }
                    if (q.kind == slang::parsing::TokenKind::StaticKeyword) {
                        isStatic = true;
                    }
                }

                if (isRand && !isStatic && prop.declaration &&
                    prop.declaration->kind == slang::syntax::SyntaxKind::DataDeclaration) {
                    const auto& data = prop.declaration->as<slang::syntax::DataDeclarationSyntax>();
                    for (const auto* decl : data.declarators) {
                        randProps[std::string(decl->name.valueText())] = decl->name.location();
                    }
                }
            }
        }

        // 2. Collect identifiers used across all constraints in the class
        ConstraintIdentifierCollector idCollector;
        for (const auto* member : cls.items) {
            if (member->kind == slang::syntax::SyntaxKind::ConstraintDeclaration) {
                member->visit(idCollector);
            }
        }

        // 3. Flag rand variables never mentioned in any constraint
        for (const auto& [propName, loc] : randProps) {
            if (idCollector.identifiers.count(propName) == 0) {
                diags.addViolation(
                    "SV-RND-015",
                    "UnconstrainedRandVariable",
                    "Class property '" + propName + "' in class '" + className +
                    "' is declared 'rand' but is never referenced in any constraint block. "
                    "Unconstrained random properties generate uniformly unguided values across the entire bit-width, increasing solver cycles and frequently indicating forgotten stimulus specifications.",
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

class UnconstrainedRandVariable : public Rule {
public:
    std::string_view getId() const override { return "SV-RND-015"; }
    std::string_view getName() const override { return "UnconstrainedRandVariable"; }
    std::string_view getDescription() const override {
        return "Flags non-static 'rand' class properties that are never constrained in any constraint block within the class.";
    }
    std::string_view getCategory() const override { return "Random"; }
    core::Severity getDefaultSeverity() const override { return core::Severity::Warning; }

    engine::RuleTask check(engine::RuleContext& ctx) override {
        auto& diags = ctx.diags();
        UnconstrainedRandVisitor visitor(diags);
        co_await ctx.visitSyntax(visitor);
    }
};

} // namespace uvmcheck::rules::random

namespace uvmcheck::rules {
std::unique_ptr<Rule> createUnconstrainedRandVariable() {
    return std::make_unique<random::UnconstrainedRandVariable>();
}
}
