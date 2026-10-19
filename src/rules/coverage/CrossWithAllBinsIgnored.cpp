#include "uvmcheck/rules/Rule.h"
#include "uvmcheck/rules/RuleRegistry.h"
#include "slang/syntax/AllSyntax.h"
#include "slang/syntax/SyntaxTree.h"
#include "slang/syntax/SyntaxVisitor.h"
#include <string>

namespace uvmcheck::rules::coverage {

class CrossWithAllBinsIgnoredVisitor : public slang::syntax::SyntaxVisitor<CrossWithAllBinsIgnoredVisitor> {
public:
    explicit CrossWithAllBinsIgnoredVisitor(core::DiagnosticEngine& diags) : diags(diags) {}

    void handle(const slang::syntax::CoverCrossSyntax& cross) {
        if (!cross.members.empty()) {
            size_t totalBins = 0;
            size_t ignoreBins = 0;

            for (const auto* m : cross.members) {
                if (m) {
                    if (m->kind == slang::syntax::SyntaxKind::BinsSelection) {
                        const auto& sel = m->as<slang::syntax::BinsSelectionSyntax>();
                        totalBins++;
                        if (sel.keyword.kind == slang::parsing::TokenKind::IgnoreBinsKeyword) {
                            ignoreBins++;
                        }
                    } else if (m->kind == slang::syntax::SyntaxKind::CoverageBins) {
                        const auto& bin = m->as<slang::syntax::CoverageBinsSyntax>();
                        totalBins++;
                        if (bin.keyword.kind == slang::parsing::TokenKind::IgnoreBinsKeyword) {
                            ignoreBins++;
                        }
                    }
                }
            }

            if (totalBins > 0 && totalBins == ignoreBins) {
                std::string labelName = "anonymous_cross";
                if (cross.label) {
                    labelName = std::string(cross.label->name.valueText());
                }

                diags.addViolation(
                    "SV-COV-010",
                    "CrossWithAllBinsIgnored",
                    "Covergroup cross '" + labelName +
                    "' defines only 'ignore_bins' with zero positive coverage bins. Masking all cross combinations produces a degenerate 0-bin cross that cannot collect coverage closure data.",
                    core::Severity::Warning,
                    cross.cross.location()
                );
            }
        }

        visitDefault(cross);
    }

private:
    core::DiagnosticEngine& diags;
};

class CrossWithAllBinsIgnored : public Rule {
public:
    std::string_view getId() const override { return "SV-COV-010"; }
    std::string_view getName() const override { return "CrossWithAllBinsIgnored"; }
    std::string_view getDescription() const override {
        return "Flags covergroup cross constructs where all defined bins are ignore_bins, producing vacuous 0-bin coverage metrics.";
    }
    std::string_view getCategory() const override { return "Coverage"; }
    core::Severity getDefaultSeverity() const override { return core::Severity::Warning; }

    void run(slang::ast::Compilation& compilation,
             const uvmcheck::uvm::UvmModel& /*uvmModel*/,
             core::DiagnosticEngine& diags) override {
        CrossWithAllBinsIgnoredVisitor visitor(diags);
        for (const auto& tree : compilation.getSyntaxTrees()) {
            if (tree) {
                tree->root().visit(visitor);
            }
        }
    }
};

} // namespace uvmcheck::rules::coverage

namespace uvmcheck::rules {
std::unique_ptr<Rule> createCrossWithAllBinsIgnored() {
    return std::make_unique<coverage::CrossWithAllBinsIgnored>();
}
} // namespace uvmcheck::rules
