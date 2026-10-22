#include "uvmcheck/rules/Rule.h"
#include "uvmcheck/rules/RuleRegistry.h"
#include "slang/syntax/AllSyntax.h"
#include "slang/syntax/SyntaxTree.h"
#include "slang/syntax/SyntaxVisitor.h"

namespace uvmcheck::rules::coverage {

class CrossBinExplosionVisitor : public engine::SyntaxVisitor<CrossBinExplosionVisitor> {
public:
    explicit CrossBinExplosionVisitor(core::DiagnosticEngine& diags) : diags(diags) {}

    void handle(const slang::syntax::CoverCrossSyntax& cross) {
        if (cross.items.size() >= 3) {
            bool hasFilter = false;
            for (const auto* member : cross.members) {
                std::string memberStr = std::string(member->toString());
                if (memberStr.find("ignore_bins") != std::string::npos ||
                    memberStr.find("binsof") != std::string::npos ||
                    memberStr.find("illegal_bins") != std::string::npos) {
                    hasFilter = true;
                    break;
                }
            }

            if (!hasFilter) {
                std::string crossName = cross.label ? std::string(cross.label->name.valueText()) : "unnamed_cross";
                diags.addViolation(
                    "SV-COV-006",
                    "CovergroupCrossBinExplosion",
                    "Covergroup cross '" + crossName + "' crosses " + std::to_string(cross.items.size()) +
                    " coverpoints without 'ignore_bins' or 'binsof' filters. "
                    "Unfiltered 3-way or higher cross coverage creates a combinatorial explosion of auto-bins, "
                    "exhausting simulator memory and bloating coverage databases. "
                    "Apply 'ignore_bins' or 'binsof' expressions to exclude unreachable or irrelevant cross combinations.",
                    core::Severity::Error,
                    cross.cross.location()
                );
            }
        }
        visitDefault(cross);
    }

private:
    core::DiagnosticEngine& diags;
};

class CovergroupCrossBinExplosion : public Rule {
public:
    std::string_view getId() const override { return "SV-COV-006"; }
    std::string_view getName() const override { return "CovergroupCrossBinExplosion"; }
    std::string_view getDescription() const override {
        return "Disallows crossing 3 or more coverpoints without 'ignore_bins' or 'binsof' exclusion filters. "
               "Unfiltered multi-way crosses cause combinatorial auto-bin explosion, leading to simulator memory exhaustion.";
    }
    std::string_view getCategory() const override { return "Coverage"; }
    core::Severity getDefaultSeverity() const override { return core::Severity::Error; }

    engine::RuleTask check(engine::RuleContext& ctx) override {
        auto& diags = ctx.diags();
        CrossBinExplosionVisitor visitor(diags);
        co_await ctx.visitSyntax(visitor);
    }
};

} // namespace uvmcheck::rules::coverage

namespace uvmcheck::rules {
std::unique_ptr<Rule> createCovergroupCrossBinExplosion() {
    return std::make_unique<coverage::CovergroupCrossBinExplosion>();
}
}
