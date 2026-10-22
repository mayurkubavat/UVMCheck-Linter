#include "uvmcheck/rules/Rule.h"
#include "uvmcheck/rules/RuleRegistry.h"
#include "slang/syntax/AllSyntax.h"
#include "slang/syntax/SyntaxTree.h"
#include "slang/syntax/SyntaxVisitor.h"
#include <string>

namespace uvmcheck::rules::coverage {

class CoverpointDefaultBinVisitor : public engine::SyntaxVisitor<CoverpointDefaultBinVisitor> {
public:
    explicit CoverpointDefaultBinVisitor(core::DiagnosticEngine& diags) : diags(diags) {}

    void handle(const slang::syntax::CoverageBinsSyntax& binNode) {
        if (binNode.keyword.kind == slang::parsing::TokenKind::BinsKeyword) {
            if (binNode.initializer &&
                binNode.initializer->kind == slang::syntax::SyntaxKind::DefaultCoverageBinInitializer) {
                std::string binName = std::string(binNode.name.valueText());
                diags.addViolation(
                    "SV-COV-009",
                    "CoverpointDefaultBinHazard",
                    "Coverpoint bin '" + binName +
                    "' is defined with 'default' initializer. Catch-all 'default' bins group all uncharacterized stimulus values into a single bin, artificially inflating coverage closure metrics and masking unhandled corner cases. Use explicit bin ranges, 'ignore_bins', or 'illegal_bins' instead.",
                    core::Severity::Warning,
                    binNode.name.location()
                );
            }
        }

        visitDefault(binNode);
    }

private:
    core::DiagnosticEngine& diags;
};

class CoverpointDefaultBinHazard : public Rule {
public:
    std::string_view getId() const override { return "SV-COV-009"; }
    std::string_view getName() const override { return "CoverpointDefaultBinHazard"; }
    std::string_view getDescription() const override {
        return "Flags non-ignoring 'default' bins in coverpoints that mask uncovered stimulus and give a false sense of coverage closure.";
    }
    std::string_view getCategory() const override { return "Coverage"; }
    core::Severity getDefaultSeverity() const override { return core::Severity::Warning; }

    engine::RuleTask check(engine::RuleContext& ctx) override {
        auto& diags = ctx.diags();
        CoverpointDefaultBinVisitor visitor(diags);
        co_await ctx.visitSyntax(visitor);
    }
};

} // namespace uvmcheck::rules::coverage

namespace uvmcheck::rules {
std::unique_ptr<Rule> createCoverpointDefaultBinHazard() {
    return std::make_unique<coverage::CoverpointDefaultBinHazard>();
}
} // namespace uvmcheck::rules
