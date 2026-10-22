#include "uvmcheck/rules/Rule.h"
#include "uvmcheck/rules/RuleRegistry.h"
#include "slang/syntax/AllSyntax.h"
#include "slang/syntax/SyntaxTree.h"
#include "slang/syntax/SyntaxVisitor.h"
#include <string>

namespace uvmcheck::rules::random {

class InvertedRangeVisitor : public engine::SyntaxVisitor<InvertedRangeVisitor> {
public:
    explicit InvertedRangeVisitor(core::DiagnosticEngine& diags) : diags(diags) {}

    void handle(const slang::syntax::ValueRangeExpressionSyntax& range) {
        auto parseVal = [](const slang::syntax::ExpressionSyntax& expr, int64_t& out) -> bool {
            if (expr.kind == slang::syntax::SyntaxKind::IntegerLiteralExpression) {
                const auto& lit = expr.as<slang::syntax::LiteralExpressionSyntax>();
                auto sv = lit.literal.valueText();
                try {
                    out = std::stoll(std::string(sv), nullptr, 0);
                    return true;
                } catch (...) {
                    return false;
                }
            }
            return false;
        };

        int64_t low = 0, high = 0;
        if (range.left && range.right && parseVal(*range.left, low) && parseVal(*range.right, high)) {
            if (low > high) {
                diags.addViolation(
                    "SV-RND-021",
                    "ConstraintInsideInvertedRange",
                    "Range expression '[" + std::to_string(low) + " : " + std::to_string(high) +
                    "]' inside set membership specifies an inverted range where lower bound exceeds upper bound (" +
                    std::to_string(low) + " > " + std::to_string(high) + "). In SystemVerilog (IEEE 1800 § 18.5.7 and "
                    "IEEE 1800 § 18.5.4), an inverted range defines an empty set, causing "
                    "constraint solver contradictions and runtime randomization failure.",
                    core::Severity::Error,
                    range.openBracket.location()
                );
            }
        }
        visitDefault(range);
    }

private:
    core::DiagnosticEngine& diags;
};

class ConstraintInsideInvertedRange : public Rule {
public:
    std::string_view getId() const override { return "SV-RND-021"; }
    std::string_view getName() const override { return "ConstraintInsideInvertedRange"; }
    std::string_view getDescription() const override {
        return "Disallows inverted value ranges [low : high] where low > high in set membership expressions.";
    }
    std::string_view getCategory() const override { return "Randomization"; }
    core::Severity getDefaultSeverity() const override { return core::Severity::Error; }

    engine::RuleTask check(engine::RuleContext& ctx) override {
        auto& diags = ctx.diags();
        InvertedRangeVisitor visitor(diags);
        co_await ctx.visitSyntax(visitor);
    }
};

} // namespace uvmcheck::rules::random

namespace uvmcheck::rules {
std::unique_ptr<Rule> createConstraintInsideInvertedRange() {
    return std::make_unique<random::ConstraintInsideInvertedRange>();
}
} // namespace uvmcheck::rules
