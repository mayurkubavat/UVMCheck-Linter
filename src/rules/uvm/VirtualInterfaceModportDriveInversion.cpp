#include "uvmcheck/rules/Rule.h"
#include "uvmcheck/rules/RuleRegistry.h"
#include "uvmcheck/model/SlangModel.h"

namespace uvmcheck::rules::uvm {

class VirtualInterfaceModportDriveInversion : public Rule {
public:
    std::string_view getId() const override { return "UVM-VIF-DIR"; }
    std::string_view getName() const override { return "VirtualInterfaceModportDriveInversion"; }
    std::string_view getDescription() const override {
        return "Detects procedural signal driving (via '=' or '<=') on virtual interface handles inside "
               "passive uvm_monitor components. Monitors must remain purely passive observers to avoid "
               "multi-driver electrical contention and X-propagation.";
    }
    std::string_view getCategory() const override { return "UVM"; }
    core::Severity getDefaultSeverity() const override { return core::Severity::Error; }

    // Reads only uvmcheck::model, not slang's AST.
    void run(slang::ast::Compilation&,
             const uvmcheck::uvm::UvmModel& uvmModel,
             core::DiagnosticEngine& diags) override {
        const model::Model& design = uvmModel.facade();
        for (const model::Class& cls : design.classes()) {
            if (cls.isParameterized) continue;
            const bool isMonitor = cls.name.find("monitor") != std::string::npos ||
                                   design.derivesFrom(design.idOf(cls), "uvm_monitor");
            if (!isMonitor) continue;
            for (const model::Method& sub : cls.methods) {
                if (sub.isExtern) continue; // in-class bodies, as before the port
                for (const model::Assignment& assign : sub.assignments) {
                    const std::string& lhs = assign.lhs;
                    if (lhs.find("vif.") == std::string::npos && lhs.find("intf.") == std::string::npos &&
                        lhs.find("_vif.") == std::string::npos)
                        continue;
                    diags.addViolation(
                        "UVM-VIF-DIR",
                        "VirtualInterfaceModportDriveInversion",
                        "Passive uvm_monitor '" + cls.name + "' illegally drives virtual interface signal '" +
                        lhs + "'. Monitors must be strictly read-only observers. Driving signals from a monitor "
                        "violates UVM passive component semantics and causes multi-driver contention.",
                        core::Severity::Error,
                        model::toSlang(assign.loc)
                    );
                }
            }
        }
    }
};

} // namespace uvmcheck::rules::uvm

namespace uvmcheck::rules {
std::unique_ptr<Rule> createVirtualInterfaceModportDriveInversion() {
    return std::make_unique<uvm::VirtualInterfaceModportDriveInversion>();
}
}

UVMCHECK_REGISTER_RULE(uvmcheck::rules::createVirtualInterfaceModportDriveInversion)
