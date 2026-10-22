#include "uvmcheck/rules/Rule.h"
#include "uvmcheck/rules/RuleRegistry.h"
#include "slang/syntax/AllSyntax.h"
#include "slang/syntax/SyntaxTree.h"
#include "slang/syntax/SyntaxVisitor.h"
#include <string>

namespace uvmcheck::rules::uvm {

class PhaseJumpVisitor : public engine::SyntaxVisitor<PhaseJumpVisitor> {
public:
    explicit PhaseJumpVisitor(core::DiagnosticEngine& diags) : diags(diags) {}

    void handle(const slang::syntax::InvocationExpressionSyntax& inv) {
        if (inv.left) {
            std::string methodName = std::string(inv.left->getLastToken().valueText());
            if (methodName == "jump") {
                std::string callStr = std::string(inv.left->toString());
                if (callStr.find("phase") != std::string::npos ||
                    callStr.find("domain") != std::string::npos) {
                    diags.addViolation(
                        "UVM-PHS-007",
                        "UvmPhaseJumpHazard",
                        "Manual UVM phase jump via 'phase.jump()' abruptly interrupts the UVM phasing schedule. "
                        "Dynamic phase jumping causes phase objection leaks, scoreboard desynchronization, and dangling TLM transactions. "
                        "Coordinate testbench phases using standard sequences, events, or reset callbacks instead.",
                        core::Severity::Warning,
                        inv.left->getFirstToken().location()
                    );
                }
            }
        }
        visitDefault(inv);
    }

private:
    core::DiagnosticEngine& diags;
};

class UvmPhaseJumpHazard : public Rule {
public:
    std::string_view getId() const override { return "UVM-PHS-007"; }
    std::string_view getName() const override { return "UvmPhaseJumpHazard"; }
    std::string_view getDescription() const override {
        return "Disallows manual phase jumping ('phase.jump(...)') in UVM testbenches due to objection leaks and scoreboard desynchronization risks.";
    }
    std::string_view getCategory() const override { return "UVM"; }
    core::Severity getDefaultSeverity() const override { return core::Severity::Warning; }

    engine::RuleTask check(engine::RuleContext& ctx) override {
        auto& diags = ctx.diags();
        PhaseJumpVisitor visitor(diags);
        co_await ctx.visitSyntax(visitor);
    }
};

} // namespace uvmcheck::rules::uvm

namespace uvmcheck::rules {
std::unique_ptr<Rule> createUvmPhaseJumpHazard() {
    return std::make_unique<uvm::UvmPhaseJumpHazard>();
}
}
