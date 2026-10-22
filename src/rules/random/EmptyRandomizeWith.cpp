#include "uvmcheck/rules/Rule.h"
#include "uvmcheck/rules/RuleRegistry.h"
#include "slang/syntax/AllSyntax.h"
#include "slang/syntax/SyntaxTree.h"
#include "slang/syntax/SyntaxVisitor.h"
#include <string>

namespace uvmcheck::rules::random {

class EmptyRandomizeWithVisitor : public engine::SyntaxVisitor<EmptyRandomizeWithVisitor> {
public:
    explicit EmptyRandomizeWithVisitor(core::DiagnosticEngine& diags) : diags(diags) {}

    void handle(const slang::syntax::ArrayOrRandomizeMethodExpressionSyntax& expr) {
        std::string methodStr = std::string(expr.method->toString());
        if (methodStr.find("randomize") != std::string::npos && expr.constraints) {
            if (expr.constraints->items.empty()) {
                std::vector<core::FixItHint> fixIts;
                if (expr.with.range().start().valid() && expr.constraints->closeBrace.range().end().valid()) {
                    fixIts.push_back({
                        slang::SourceRange(expr.with.range().start(), expr.constraints->closeBrace.range().end()),
                        "",
                        "Remove empty 'with { }' constraint block"
                    });
                }

                diags.addViolation(
                    "SV-RND-014",
                    "EmptyRandomizeWith",
                    "Inline constraint block 'with { }' is empty. "
                    "Empty inline constraints construct unnecessary dynamic solver scopes at runtime with zero constraints added. "
                    "Remove the redundant 'with { }' block.",
                    core::Severity::Warning,
                    expr.with.location(),
                    fixIts
                );
            }
        }

        visitDefault(expr);
    }

private:
    core::DiagnosticEngine& diags;
};

class EmptyRandomizeWith : public Rule {
public:
    std::string_view getId() const override { return "SV-RND-014"; }
    std::string_view getName() const override { return "EmptyRandomizeWith"; }
    std::string_view getDescription() const override {
        return "Flags 'randomize() with {}' containing an empty inline constraint block. "
               "Empty inline constraint blocks incur dynamic solver scope allocation overhead while adding no constraints.";
    }
    std::string_view getCategory() const override { return "Random"; }
    core::Severity getDefaultSeverity() const override { return core::Severity::Warning; }

    engine::RuleTask check(engine::RuleContext& ctx) override {
        auto& diags = ctx.diags();
        EmptyRandomizeWithVisitor visitor(diags);
        co_await ctx.visitSyntax(visitor);
    }
};

} // namespace uvmcheck::rules::random

namespace uvmcheck::rules {
std::unique_ptr<Rule> createEmptyRandomizeWith() {
    return std::make_unique<random::EmptyRandomizeWith>();
}
}
