#include "uvmcheck/rules/Rule.h"
#include "uvmcheck/rules/RuleRegistry.h"
#include "uvmcheck/model/SlangModel.h"

namespace uvmcheck::rules::uvm {

class UvmUnpairedObjection : public Rule {
public:
    std::string_view getId() const override { return "UVM-OBJ-001"; }
    std::string_view getName() const override { return "UvmUnpairedObjection"; }
    std::string_view getDescription() const override {
        return "Every phase.raise_objection() call must have a corresponding phase.drop_objection() "
               "within the same execution context to prevent simulation hangs or fatal errors.";
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
            if (!model::isComponent(cls.uvm) && cls.uvm != model::UvmKind::Sequence) continue;

            for (const model::Method& sub : cls.methods) {
                if (sub.isExtern || sub.isBuiltIn) continue;
                const model::Call* firstRaise = nullptr;
                const model::Call* firstDrop = nullptr;
                for (const model::Call& call : sub.calls) {
                    if (call.name == "raise_objection" && !firstRaise) firstRaise = &call;
                    else if (call.name == "drop_objection" && !firstDrop) firstDrop = &call;
                }

                // Case 1: Raised but never dropped in this subroutine
                if (firstRaise && !firstDrop) {
                    diags.addViolation(
                        "UVM-OBJ-001",
                        "UvmUnpairedObjection",
                        "Method '" + sub.name + "' in class '" + cls.name +
                        "' calls 'raise_objection' but has no corresponding 'drop_objection'. "
                        "This un-dropped objection will cause the simulation to hang indefinitely.",
                        core::Severity::Error,
                        model::toSlang(firstRaise->loc)
                    );
                }
                // Case 2: Dropped but never raised in this subroutine
                else if (firstDrop && !firstRaise) {
                    diags.addViolation(
                        "UVM-OBJ-001",
                        "UvmUnpairedObjection",
                        "Method '" + sub.name + "' in class '" + cls.name +
                        "' calls 'drop_objection' without any prior 'raise_objection' in the same scope. "
                        "Dropping unraised objections leads to UVM runtime fatal errors.",
                        core::Severity::Error,
                        model::toSlang(firstDrop->loc)
                    );
                }
            }
        }
    }
};

} // namespace uvmcheck::rules::uvm

namespace uvmcheck::rules {
std::unique_ptr<Rule> createUvmUnpairedObjection() {
    return std::make_unique<uvm::UvmUnpairedObjection>();
}
}

UVMCHECK_REGISTER_RULE(uvmcheck::rules::createUvmUnpairedObjection)
