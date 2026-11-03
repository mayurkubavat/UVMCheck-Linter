#include "uvmcheck/rules/Rule.h"
#include "uvmcheck/rules/RuleRegistry.h"
#include "uvmcheck/model/SlangModel.h"

namespace uvmcheck::rules::uvm {

// Assignments made in any connect_phase, in any class (extern bodies and
// parameterized classes included): "lhs = rhs" as written.
inline std::vector<std::string> connectPhaseAssignments(const model::Model& design) {
    std::vector<std::string> out;
    for (const model::Class& cls : design.classes())
        for (const model::Method& m : cls.methods)
            if (m.name == "connect_phase")
                for (const model::Assignment& a : m.assignments) out.push_back(a.lhs + " = " + a.rhs);
    return out;
}

class VirtualSequencerWiringChecker : public Rule {
public:
    std::string_view getId() const override { return "UVM-VSEQ-001"; }
    std::string_view getName() const override { return "VirtualSequencerWiringChecker"; }
    std::string_view getDescription() const override {
        return "Statically verifies that sub-sequencer handles declared within a virtual sequencer "
               "are fully wired and assigned in connect_phase across the verification environment.";
    }
    std::string_view getCategory() const override { return "UVM"; }
    core::Severity getDefaultSeverity() const override { return core::Severity::Warning; }

    // Reads only uvmcheck::model, not slang's AST.
    void run(slang::ast::Compilation&,
             const uvmcheck::uvm::UvmModel& uvmModel,
             core::DiagnosticEngine& diags) override {
        const model::Model& design = uvmModel.facade();
        struct SubSeqrField {
            const model::Class* owner;
            const model::Property* prop;
        };
        std::vector<SubSeqrField> fields;
        for (const model::Class& cls : design.classes()) {
            if (cls.isParameterized) continue;
            const bool isVirtualSeqr = cls.name.find("vseqr") != std::string::npos ||
                                       cls.name.find("virtual_sequencer") != std::string::npos ||
                                       design.derivesFrom(design.idOf(cls), "uvm_sequencer");
            if (!isVirtualSeqr) continue;
            for (const model::Property& prop : cls.properties) {
                const bool isSubSequencer = prop.type.find("sequencer") != std::string::npos ||
                                            prop.name.find("_seqr") != std::string::npos ||
                                            prop.name.find("sequencer") != std::string::npos;
                if (isSubSequencer && !design.derivesFrom(prop.typeClass, "uvm_phase"))
                    fields.push_back({&cls, &prop});
            }
        }
        if (fields.empty()) return;

        const auto assignments = connectPhaseAssignments(design);
        for (const auto& field : fields) {
            bool assigned = false;
            for (const auto& stmt : assignments) {
                if (stmt.find(field.prop->name) != std::string::npos) {
                    assigned = true;
                    break;
                }
            }
            if (assigned) continue;
            diags.addViolation(
                "UVM-VSEQ-001",
                "VirtualSequencerWiringChecker",
                "Sub-sequencer handle '" + field.prop->name + "' in virtual sequencer '" +
                field.owner->name + "' is declared but never wired in any connect_phase. "
                "Virtual sequences attempting to access p_sequencer." + field.prop->name +
                " will crash with a null-pointer dereference at runtime.",
                core::Severity::Warning,
                model::toSlang(field.prop->loc)
            );
        }
    }
};

} // namespace uvmcheck::rules::uvm

namespace uvmcheck::rules {
std::unique_ptr<Rule> createVirtualSequencerWiringChecker() {
    return std::make_unique<uvm::VirtualSequencerWiringChecker>();
}
}

UVMCHECK_REGISTER_RULE(uvmcheck::rules::createVirtualSequencerWiringChecker)
