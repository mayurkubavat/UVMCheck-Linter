#include "uvmcheck/rules/Rule.h"
#include "uvmcheck/rules/RuleRegistry.h"
#include "slang/syntax/AllSyntax.h"
#include "slang/syntax/SyntaxTree.h"
#include "slang/syntax/SyntaxVisitor.h"
#include <unordered_map>
#include <string>
#include <vector>

namespace uvmcheck::rules::coverage {

struct BinValueInfo {
    std::string binName;
    slang::SourceLocation location;
};

static void extractBinValues(const slang::syntax::CoverageBinInitializerSyntax& init,
                             const std::string& binName,
                             std::unordered_map<std::string, BinValueInfo>& outValues) {
    if (init.kind == slang::syntax::SyntaxKind::RangeCoverageBinInitializer) {
        const auto& rInit = init.as<slang::syntax::RangeCoverageBinInitializerSyntax>();
        if (rInit.ranges) {
            for (const auto* expr : rInit.ranges->valueRanges) {
                if (expr) {
                    std::string valStr = std::string(expr->getFirstToken().valueText());
                    if (valStr.empty()) {
                        valStr = expr->toString();
                    }
                    outValues[valStr] = { binName, expr->getFirstToken().location() };
                }
            }
        }
    }
}

class IgnoreIllegalOverlapVisitor : public slang::syntax::SyntaxVisitor<IgnoreIllegalOverlapVisitor> {
public:
    explicit IgnoreIllegalOverlapVisitor(core::DiagnosticEngine& diags) : diags(diags) {}

    void handle(const slang::syntax::CoverpointSyntax& cp) {
        std::string cpName = cp.label ? std::string(cp.label->name.valueText()) : "<unnamed_coverpoint>";

        std::unordered_map<std::string, BinValueInfo> ignoreValues;
        std::unordered_map<std::string, BinValueInfo> illegalValues;

        for (const auto* member : cp.members) {
            if (member && member->kind == slang::syntax::SyntaxKind::CoverageBins) {
                const auto& bins = member->as<slang::syntax::CoverageBinsSyntax>();
                std::string kw = std::string(bins.keyword.valueText());
                std::string bName = std::string(bins.name.valueText());

                if (kw == "ignore_bins") {
                    extractBinValues(*bins.initializer, bName, ignoreValues);
                } else if (kw == "illegal_bins") {
                    extractBinValues(*bins.initializer, bName, illegalValues);
                }
            }
        }

        // Check for any overlap between ignore_bins and illegal_bins
        for (const auto& [val, illInfo] : illegalValues) {
            auto it = ignoreValues.find(val);
            if (it != ignoreValues.end()) {
                diags.addViolation(
                    "SV-COV-013",
                    "IgnoreBinsOverlapsIllegalBins",
                    "Coverage value '" + val + "' is specified in both 'ignore_bins' (" + it->second.binName +
                    ") and 'illegal_bins' (" + illInfo.binName + ") within coverpoint '" + cpName + "'. "
                    "In IEEE 1800 § 19.5 coverage methodology, overlapping "
                    "ignore and illegal bins creates contradictory coverage semantics and causes simulator runtime errors.",
                    core::Severity::Error,
                    illInfo.location
                );
            }
        }

        visitDefault(cp);
    }

private:
    core::DiagnosticEngine& diags;
};

class IgnoreBinsOverlapsIllegalBins : public Rule {
public:
    std::string_view getId() const override { return "SV-COV-013"; }
    std::string_view getName() const override { return "IgnoreBinsOverlapsIllegalBins"; }
    std::string_view getDescription() const override {
        return "Flags coverage models where the same value or range is specified in both 'ignore_bins' and 'illegal_bins' within a coverpoint.";
    }
    std::string_view getCategory() const override { return "Coverage"; }
    core::Severity getDefaultSeverity() const override { return core::Severity::Error; }

    void run(slang::ast::Compilation& compilation,
             const uvmcheck::uvm::UvmModel& /*uvmModel*/,
             core::DiagnosticEngine& diags) override {
        IgnoreIllegalOverlapVisitor visitor(diags);
        for (const auto& tree : compilation.getSyntaxTrees()) {
            if (tree) {
                tree->root().visit(visitor);
            }
        }
    }
};

} // namespace uvmcheck::rules::coverage

namespace uvmcheck::rules {
std::unique_ptr<Rule> createIgnoreBinsOverlapsIllegalBins() {
    return std::make_unique<coverage::IgnoreBinsOverlapsIllegalBins>();
}
} // namespace uvmcheck::rules
