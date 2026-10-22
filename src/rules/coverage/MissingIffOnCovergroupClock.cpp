#include "uvmcheck/rules/Rule.h"
#include "uvmcheck/rules/RuleRegistry.h"
#include "slang/syntax/AllSyntax.h"
#include "slang/syntax/SyntaxTree.h"
#include "slang/syntax/SyntaxVisitor.h"
#include <string>

namespace uvmcheck::rules::coverage {

class CovergroupClockVisitor : public engine::SyntaxVisitor<CovergroupClockVisitor> {
public:
    explicit CovergroupClockVisitor(core::DiagnosticEngine& diags) : diags(diags) {}

    void handle(const slang::syntax::CovergroupDeclarationSyntax& cg) {
        if (cg.event) {
            std::string evStr = std::string(cg.event->toString());
            // If sampled by a clocking event (@(posedge clk) or @(...)) without an iff condition
            if (evStr.find('@') != std::string::npos && evStr.find("iff") == std::string::npos) {
                std::string cgName = std::string(cg.name.valueText());
                diags.addViolation(
                    "SV-COV-015",
                    "MissingIffOnCovergroupClock",
                    "Covergroup '" + cgName + "' is sampled by clocking event '" + evStr +
                    "' without an 'iff (!reset)' guard condition. In functional coverage methodology "
                    "(IEEE 1800 § 19.4), clock-sampled covergroups should include an 'iff' "
                    "guard to disable sampling during active reset, preventing reset-state transactions from contaminating closure metrics.",
                    core::Severity::Warning,
                    cg.event->getFirstToken().location()
                );
            }
        }
        visitDefault(cg);
    }

private:
    core::DiagnosticEngine& diags;
};

class MissingIffOnCovergroupClock : public Rule {
public:
    std::string_view getId() const override { return "SV-COV-015"; }
    std::string_view getName() const override { return "MissingIffOnCovergroupClock"; }
    std::string_view getDescription() const override {
        return "Covergroups sampled by a clocking event should specify an 'iff' reset guard condition.";
    }
    std::string_view getCategory() const override { return "Coverage"; }
    core::Severity getDefaultSeverity() const override { return core::Severity::Warning; }

    engine::RuleTask check(engine::RuleContext& ctx) override {
        auto& diags = ctx.diags();
        CovergroupClockVisitor visitor(diags);
        co_await ctx.visitSyntax(visitor);
    }
};

} // namespace uvmcheck::rules::coverage

namespace uvmcheck::rules {
std::unique_ptr<Rule> createMissingIffOnCovergroupClock() {
    return std::make_unique<coverage::MissingIffOnCovergroupClock>();
}
} // namespace uvmcheck::rules
