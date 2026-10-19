#include "uvmcheck/rules/Rule.h"
#include "uvmcheck/rules/RuleRegistry.h"
#include "slang/syntax/AllSyntax.h"
#include "slang/syntax/SyntaxTree.h"
#include "slang/syntax/SyntaxVisitor.h"
#include <string>
#include <vector>
#include <algorithm>

namespace uvmcheck::rules::coverage {

class OverlappingBinRangeVisitor : public slang::syntax::SyntaxVisitor<OverlappingBinRangeVisitor> {
public:
    explicit OverlappingBinRangeVisitor(core::DiagnosticEngine& diags) : diags(diags) {}

    void handle(const slang::syntax::CoverageBinsSyntax& bins) {
        struct RangeCollector : public slang::syntax::SyntaxVisitor<RangeCollector> {
            std::vector<std::pair<int64_t, int64_t>> ranges;

            void handle(const slang::syntax::ValueRangeExpressionSyntax& range) {
                auto parseVal = [](const slang::syntax::ExpressionSyntax& expr, int64_t& out) -> bool {
                    if (expr.kind == slang::syntax::SyntaxKind::IntegerLiteralExpression) {
                        const auto& lit = expr.as<slang::syntax::LiteralExpressionSyntax>();
                        try {
                            out = std::stoll(std::string(lit.literal.valueText()), nullptr, 0);
                            return true;
                        } catch (...) {
                            return false;
                        }
                    }
                    return false;
                };

                int64_t low = 0, high = 0;
                if (range.left && range.right && parseVal(*range.left, low) && parseVal(*range.right, high)) {
                    if (low <= high) {
                        ranges.emplace_back(low, high);
                    }
                }
                visitDefault(range);
            }
        };

        RangeCollector collector;
        bins.visit(collector);

        // Check for any pairwise overlap
        for (size_t i = 0; i < collector.ranges.size(); ++i) {
            for (size_t j = i + 1; j < collector.ranges.size(); ++j) {
                auto r1 = collector.ranges[i];
                auto r2 = collector.ranges[j];
                if (std::max(r1.first, r2.first) <= std::min(r1.second, r2.second)) {
                    std::string binName = std::string(bins.name.valueText());
                    diags.addViolation(
                        "SV-COV-016",
                        "OverlappingRangesInCoverpointBin",
                        "Coverpoint bin '" + binName + "' contains overlapping value ranges '[" +
                        std::to_string(r1.first) + ":" + std::to_string(r1.second) + "]' and '[" +
                        std::to_string(r2.first) + ":" + std::to_string(r2.second) + "]'. In functional coverage "
                        "methodology (IEEE 1800 § 19.5), overlapping sub-ranges within a single "
                        "bin definition are redundant and indicate specification errors in the functional coverage model.",
                        core::Severity::Warning,
                        bins.name.location()
                    );
                    return; // Report once per bin
                }
            }
        }

        visitDefault(bins);
    }

private:
    core::DiagnosticEngine& diags;
};

class OverlappingRangesInCoverpointBin : public Rule {
public:
    std::string_view getId() const override { return "SV-COV-016"; }
    std::string_view getName() const override { return "OverlappingRangesInCoverpointBin"; }
    std::string_view getDescription() const override {
        return "Flags coverpoint bins containing overlapping numerical value ranges.";
    }
    std::string_view getCategory() const override { return "Coverage"; }
    core::Severity getDefaultSeverity() const override { return core::Severity::Warning; }

    void run(slang::ast::Compilation& compilation,
             const uvmcheck::uvm::UvmModel& /*uvmModel*/,
             core::DiagnosticEngine& diags) override {
        OverlappingBinRangeVisitor visitor(diags);
        for (const auto& tree : compilation.getSyntaxTrees()) {
            if (tree) {
                tree->root().visit(visitor);
            }
        }
    }
};

} // namespace uvmcheck::rules::coverage

namespace uvmcheck::rules {
std::unique_ptr<Rule> createOverlappingRangesInCoverpointBin() {
    return std::make_unique<coverage::OverlappingRangesInCoverpointBin>();
}
} // namespace uvmcheck::rules
