#include "uvmcheck/rules/Rule.h"
#include "uvmcheck/rules/RuleRegistry.h"
#include "slang/syntax/AllSyntax.h"
#include "slang/syntax/SyntaxTree.h"
#include "slang/syntax/SyntaxVisitor.h"

namespace uvmcheck::rules::random {

class StaticConstraintVisitor : public engine::SyntaxVisitor<StaticConstraintVisitor> {
public:
    explicit StaticConstraintVisitor(core::DiagnosticEngine& diags) : diags(diags) {}

    void handle(const slang::syntax::ConstraintDeclarationSyntax& node) {
        for (const auto& qual : node.qualifiers) {
            if (qual.kind == slang::parsing::TokenKind::StaticKeyword) {
                std::vector<core::FixItHint> fixIts;
                if (qual.range().start().valid()) {
                    fixIts.push_back({
                        qual.range(),
                        "",
                        "Remove 'static' keyword to make constraint instance-specific"
                    });
                }

                std::string constraintName = node.name ? std::string(node.name->toString()) : "unnamed";
                // Trim whitespace
                constraintName.erase(0, constraintName.find_first_not_of(" \t\n\r"));
                if (size_t end = constraintName.find_last_not_of(" \t\n\r"); end != std::string::npos) {
                    constraintName.erase(end + 1);
                }

                diags.addViolation(
                    "SV-RND-009",
                    "StaticConstraint",
                    "Constraint '" + constraintName +
                    "' is declared 'static'. In SystemVerilog (IEEE 1800 § 18.5.2), static constraints are shared across all instances of the class; "
                    "dynamically disabling or altering them creates cross-instance state pollution and non-deterministic test failures.",
                    core::Severity::Warning,
                    qual.location(),
                    std::move(fixIts)
                );
                break;
            }
        }
        visitDefault(node);
    }

private:
    core::DiagnosticEngine& diags;
};

class StaticConstraint : public Rule {
public:
    std::string_view getId() const override { return "SV-RND-009"; }
    std::string_view getName() const override { return "StaticConstraint"; }
    std::string_view getDescription() const override {
        return "Disallows static constraints. Static constraints share solver state across all class instances in the simulation, "
               "causing hidden dependencies and test flakiness.";
    }
    std::string_view getCategory() const override { return "Random"; }
    core::Severity getDefaultSeverity() const override { return core::Severity::Warning; }

    engine::RuleTask check(engine::RuleContext& ctx) override {
        auto& diags = ctx.diags();
        StaticConstraintVisitor visitor(diags);
        co_await ctx.visitSyntax(visitor);
    }
};

} // namespace uvmcheck::rules::random

namespace uvmcheck::rules {
std::unique_ptr<Rule> createStaticConstraint() {
    return std::make_unique<random::StaticConstraint>();
}
}
