#include "uvmcheck/rules/Rule.h"
#include "uvmcheck/rules/RuleRegistry.h"
#include "uvmcheck/uvm/TlmTopologyGraph.h"

namespace uvmcheck::rules::uvm {

class TlmTopologyChecker : public Rule {
public:
    std::string_view getId() const override { return "UVM-TLM-004"; }
    std::string_view getName() const override { return "TlmTopologyChecker"; }
    std::string_view getDescription() const override {
        return "Performs cross-component elaboration topology checks to verify that mandatory TLM ports "
               "(such as driver seq_item_pull_port and monitor analysis_port) are bound in connect_phase "
               "across the verification environment hierarchy.";
    }
    std::string_view getCategory() const override { return "UVM"; }
    core::Severity getDefaultSeverity() const override { return core::Severity::Warning; }

    void run(slang::ast::Compilation& compilation,
             const uvmcheck::uvm::UvmModel& uvmModel,
             core::DiagnosticEngine& diags) override {
        uvmcheck::uvm::TlmTopologyGraph graph(compilation, uvmModel);
        auto unconnected = graph.findUnconnectedPorts();

        for (const auto& port : unconnected) {
            std::string portKind = port.isPullPort ? "Driver pull-port" : "Analysis port";
            diags.addViolation(
                "UVM-TLM-004",
                "TlmTopologyChecker",
                portKind + " '" + port.portName + "' in component '" + port.componentClass +
                "' is declared but never connected to any export or sequencer in connect_phase across the environment. "
                "Unconnected ports will cause null-pointer dereferences or dropped verification transactions.",
                core::Severity::Warning,
                port.location
            );
        }
    }
};

} // namespace uvmcheck::rules::uvm

namespace uvmcheck::rules {
std::unique_ptr<Rule> createTlmTopologyChecker() {
    return std::make_unique<uvm::TlmTopologyChecker>();
}
}
