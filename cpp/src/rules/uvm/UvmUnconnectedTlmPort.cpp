#include "uvmcheck/rules/Rule.h"
#include "uvmcheck/rules/RuleRegistry.h"
#include "uvmcheck/model/SlangModel.h"

namespace uvmcheck::rules::uvm {

class UvmUnconnectedTlmPort : public Rule {
public:
    std::string_view getId() const override { return "UVM-TLM-001"; }
    std::string_view getName() const override { return "UvmUnconnectedTlmPort"; }
    std::string_view getDescription() const override {
        return "Mandatory TLM ports and exports declared in uvm_component must be connected "
               "in connect_phase to prevent runtime fatal errors or null-pointer dereferences.";
    }
    std::string_view getCategory() const override { return "UVM"; }
    core::Severity getDefaultSeverity() const override { return core::Severity::Warning; }

    // Reads only uvmcheck::model, not slang's AST.
    void run(slang::ast::Compilation&,
             const uvmcheck::uvm::UvmModel& uvmModel,
             core::DiagnosticEngine& diags) override {
        const model::Model& design = uvmModel.facade();
        for (const model::Class& cls : design.classes()) {
            if (cls.isParameterized) continue;
            if (!model::isComponent(cls.uvm)) continue;
            std::vector<const model::Property*> tlmPorts;
            for (const model::Property& prop : cls.properties) {
                if (prop.type.find("_port") != std::string::npos || prop.type.find("_export") != std::string::npos)
                    tlmPorts.push_back(&prop);
            }
            if (tlmPorts.empty()) continue;

            // Receivers of x.connect(...) calls: "ap", "m_mon.ap", ...
            std::vector<std::string_view> connected;
            for (const model::Method& sub : cls.methods) {
                if (sub.isExtern || sub.isBuiltIn) continue;
                for (const model::Call& call : sub.calls)
                    if (call.name == "connect" && !call.receiver.empty()) connected.push_back(call.receiver);
            }

            for (const model::Property* port : tlmPorts) {
                bool wasConnected = false;
                for (std::string_view receiver : connected) {
                    if (receiver.ends_with(port->name)) {
                        wasConnected = true;
                        break;
                    }
                }
                if (wasConnected) continue;
                diags.addViolation(
                    "UVM-TLM-001",
                    "UvmUnconnectedTlmPort",
                    "TLM port/export '" + port->name + "' in component '" +
                    cls.name + "' is declared but never connected via .connect(...). "
                    "Unconnected TLM ports can cause simulation fatal errors upon transaction transfer.",
                    core::Severity::Warning,
                    model::toSlang(port->loc)
                );
            }
        }
    }
};

} // namespace uvmcheck::rules::uvm

namespace uvmcheck::rules {
std::unique_ptr<Rule> createUvmUnconnectedTlmPort() {
    return std::make_unique<uvm::UvmUnconnectedTlmPort>();
}
}

UVMCHECK_REGISTER_RULE(uvmcheck::rules::createUvmUnconnectedTlmPort)
