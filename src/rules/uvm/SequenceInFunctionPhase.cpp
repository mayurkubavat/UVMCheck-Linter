#include "uvmcheck/rules/Rule.h"
#include "uvmcheck/rules/RuleRegistry.h"
#include "slang/syntax/SyntaxVisitor.h"
#include "slang/syntax/AllSyntax.h"
#include "slang/syntax/SyntaxTree.h"

namespace uvmcheck::rules::uvm {

class SeqInFunctionPhaseVisitor : public engine::SyntaxVisitor<SeqInFunctionPhaseVisitor> {
public:
    explicit SeqInFunctionPhaseVisitor(core::DiagnosticEngine& diags) : diags(diags) {}

    void handle(const slang::syntax::FunctionDeclarationSyntax& routine) {
        if (routine.prototype->keyword.kind == slang::parsing::TokenKind::FunctionKeyword) {
            std::string fnName = std::string(routine.prototype->name->getFirstToken().valueText());
            static const std::unordered_set<std::string_view> nonTimePhases = {
                "build_phase",
                "connect_phase",
                "end_of_elaboration_phase",
                "start_of_simulation_phase",
                "extract_phase",
                "check_phase",
                "report_phase",
                "final_phase"
            };

            if (nonTimePhases.count(fnName) > 0) {
                std::string bodyText = std::string(routine.toString());
                if (bodyText.find(".start(") != std::string::npos ||
                    bodyText.find(".start (") != std::string::npos) {
                    diags.addViolation(
                        "UVM-PHS-004",
                        "SequenceInFunctionPhase",
                        "Calling sequence method '.start()' inside non-time-consuming function phase '" + fnName +
                        "' is illegal in UVM (IEEE 1800.2 § 9.3). Function phases execute in zero simulation time, "
                        "whereas sequence execution requires time-consuming clocking events and driver handshakes. "
                        "Sequences must only be started in task phases such as 'run_phase' or 'main_phase'.",
                        core::Severity::Error,
                        routine.prototype->name->sourceRange().start()
                    );
                }
            }
        }
        visitDefault(routine);
    }

private:
    core::DiagnosticEngine& diags;
};

class SequenceInFunctionPhase : public Rule {
public:
    std::string_view getId() const override { return "UVM-PHS-004"; }
    std::string_view getName() const override { return "SequenceInFunctionPhase"; }
    std::string_view getDescription() const override {
        return "Disallow starting sequences (.start()) inside zero-time function phases (build_phase, connect_phase, etc.). "
               "Sequences require time-consuming simulation events and must be executed in task phases.";
    }
    std::string_view getCategory() const override { return "UVM"; }
    core::Severity getDefaultSeverity() const override { return core::Severity::Error; }

    engine::RuleTask check(engine::RuleContext& ctx) override {
        auto& diags = ctx.diags();
        SeqInFunctionPhaseVisitor visitor(diags);
        co_await ctx.visitSyntax(visitor);
    }
};

} // namespace uvmcheck::rules::uvm

namespace uvmcheck::rules {
std::unique_ptr<Rule> createSequenceInFunctionPhase() {
    return std::make_unique<uvm::SequenceInFunctionPhase>();
}
}
