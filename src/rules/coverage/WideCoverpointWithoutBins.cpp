#include "uvmcheck/rules/Rule.h"
#include "uvmcheck/rules/RuleRegistry.h"
#include "slang/syntax/AllSyntax.h"
#include "slang/syntax/SyntaxTree.h"
#include "slang/syntax/SyntaxVisitor.h"
#include <string>

namespace uvmcheck::rules::coverage {

class WideCoverpointVisitor : public engine::SyntaxVisitor<WideCoverpointVisitor> {
public:
    explicit WideCoverpointVisitor(core::DiagnosticEngine& diags) : diags(diags) {}

    void handle(const slang::syntax::CoverpointSyntax& cp) {
        std::string cpName = cp.label ? std::string(cp.label->name.valueText()) : "unnamed_cp";
        std::string exprStr;
        if (cp.expr) {
            exprStr = std::string(cp.expr->toString());
            exprStr.erase(0, exprStr.find_first_not_of(" \t\n\r"));
            if (size_t end = exprStr.find_last_not_of(" \t\n\r"); end != std::string::npos) {
                exprStr.erase(end + 1);
            }
        }

        size_t binCount = 0;
        bool hasAutoBinMax = false;

        for (const auto* member : cp.members) {
            if (!member) continue;
            if (member->kind == slang::syntax::SyntaxKind::CoverageBins) {
                binCount++;
            } else if (member->kind == slang::syntax::SyntaxKind::CoverageOption) {
                std::string optText = std::string(member->toString());
                if (optText.find("auto_bin_max") != std::string::npos) {
                    hasAutoBinMax = true;
                }
            }
        }

        if (binCount == 0 && !hasAutoBinMax) {
            // Flag coverpoints that omit explicit bins and auto_bin_max
            diags.addViolation(
                "SV-COV-012",
                "WideCoverpointWithoutBins",
                "Coverpoint '" + cpName + "' on expression '" + exprStr + "' declares zero explicit bins and lacks 'option.auto_bin_max'. "
                "In SystemVerilog (IEEE 1800 § 19.5), coverpoints without explicit bins partition the state space into a default of 64 auto-bins. "
                "For wide vectors or integer types, this creates misleading coverage metrics where 64 arbitrary hits yield 100% closure across millions of untested values. "
                "Define explicit bins or configure 'option.auto_bin_max'.",
                core::Severity::Warning,
                cp.sourceRange().start()
            );
        }

        visitDefault(cp);
    }

private:
    core::DiagnosticEngine& diags;
};

class WideCoverpointWithoutBins : public Rule {
public:
    std::string_view getId() const override { return "SV-COV-012"; }
    std::string_view getName() const override { return "WideCoverpointWithoutBins"; }
    std::string_view getDescription() const override {
        return "Flags coverpoints that define no explicit bins and omit option.auto_bin_max. "
               "Default auto-binning creates 64 bins, producing misleading 100% coverage closure on wide vectors.";
    }
    std::string_view getCategory() const override { return "Coverage"; }
    core::Severity getDefaultSeverity() const override { return core::Severity::Warning; }

    engine::RuleTask check(engine::RuleContext& ctx) override {
        auto& diags = ctx.diags();
        WideCoverpointVisitor visitor(diags);
        co_await ctx.visitSyntax(visitor);
    }
};

} // namespace uvmcheck::rules::coverage

namespace uvmcheck::rules {
std::unique_ptr<Rule> createWideCoverpointWithoutBins() {
    return std::make_unique<coverage::WideCoverpointWithoutBins>();
}
}
