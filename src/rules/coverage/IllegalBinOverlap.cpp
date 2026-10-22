#include "uvmcheck/rules/Rule.h"
#include "uvmcheck/rules/RuleRegistry.h"
#include "slang/syntax/AllSyntax.h"
#include "slang/syntax/SyntaxTree.h"
#include "slang/syntax/SyntaxVisitor.h"
#include <algorithm>
#include <cctype>
#include <unordered_map>
#include <unordered_set>
#include <vector>

namespace uvmcheck::rules::coverage {

class IllegalBinOverlapVisitor : public engine::SyntaxVisitor<IllegalBinOverlapVisitor> {
public:
    explicit IllegalBinOverlapVisitor(core::DiagnosticEngine& diags) : diags(diags) {}

    void handle(const slang::syntax::CoverpointSyntax& cp) {
        std::string cpName = cp.label ? std::string(cp.label->name.valueText()) : "unnamed_cp";

        struct BinInfo {
            std::string name;
            std::string kind; // "bins", "illegal_bins", "ignore_bins"
            std::unordered_set<std::string> values;
            slang::SourceLocation loc;
        };

        std::vector<BinInfo> regularBins;
        std::vector<BinInfo> specialBins;

        for (const auto* member : cp.members) {
            if (!member) continue;
            if (const auto* bin = member->as_if<slang::syntax::CoverageBinsSyntax>()) {
                std::string kw = std::string(bin->keyword.valueText());
                std::string bName = bin->name ? std::string(bin->name.valueText()) : "unnamed";
                slang::SourceLocation loc = bin->sourceRange().start();

                std::unordered_set<std::string> values;
                if (const auto* rInit = bin->initializer->as_if<slang::syntax::RangeCoverageBinInitializerSyntax>()) {
                    if (rInit->ranges) {
                        for (const auto* expr : rInit->ranges->valueRanges) {
                            if (!expr) continue;
                            std::string valStr = std::string(expr->toString());
                            valStr.erase(0, valStr.find_first_not_of(" \t\n\r"));
                            if (size_t end = valStr.find_last_not_of(" \t\n\r"); end != std::string::npos) {
                                valStr.erase(end + 1);
                            }
                            if (!valStr.empty()) {
                                values.insert(valStr);
                            }
                        }
                    }
                }

                if (kw == "bins") {
                    regularBins.push_back({ bName, kw, std::move(values), loc });
                } else if (kw == "illegal_bins" || kw == "ignore_bins") {
                    specialBins.push_back({ bName, kw, std::move(values), loc });
                }
            }
        }

        // Check for overlaps between regularBins and specialBins
        for (const auto& sBin : specialBins) {
            for (const auto& rBin : regularBins) {
                for (const auto& sVal : sBin.values) {
                    if (rBin.values.count(sVal)) {
                        diags.addViolation(
                            "SV-COV-004",
                            "IllegalBinOverlap",
                            "Coverpoint '" + cpName + "': overlapping value '" + sVal + "' detected between regular bins '" +
                            rBin.name + "' and " + sBin.kind + " '" + sBin.name +
                            "'. In SystemVerilog (IEEE 1800 § 19.5), regular bins must not overlap with illegal_bins or ignore_bins, creating non-deterministic bin sampling and simulator errors.",
                            core::Severity::Error,
                            sBin.loc
                        );
                        break;
                    }
                }
            }
        }

        visitDefault(cp);
    }

private:
    core::DiagnosticEngine& diags;
};

class IllegalBinOverlap : public Rule {
public:
    std::string_view getId() const override { return "SV-COV-004"; }
    std::string_view getName() const override { return "IllegalBinOverlap"; }
    std::string_view getDescription() const override {
        return "Detects overlapping values between regular 'bins' and 'illegal_bins' or 'ignore_bins' within a coverpoint. "
               "Per IEEE 1800 § 19.5, regular bins must not overlap with illegal or ignore bins.";
    }
    std::string_view getCategory() const override { return "Coverage"; }
    core::Severity getDefaultSeverity() const override { return core::Severity::Error; }

    engine::RuleTask check(engine::RuleContext& ctx) override {
        auto& diags = ctx.diags();
        IllegalBinOverlapVisitor visitor(diags);
        co_await ctx.visitSyntax(visitor);
    }
};

} // namespace uvmcheck::rules::coverage

namespace uvmcheck::rules {
std::unique_ptr<Rule> createIllegalBinOverlap() {
    return std::make_unique<coverage::IllegalBinOverlap>();
}
}
