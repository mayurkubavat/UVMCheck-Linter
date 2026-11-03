#include "uvmcheck/rules/Rule.h"
#include "uvmcheck/rules/RuleRegistry.h"
#include "uvmcheck/model/SlangModel.h"

namespace uvmcheck::rules::uvm {

class ObjectionInNonRunPhase : public Rule {
public:
    std::string_view getId() const override { return "UVM-PHS-009"; }
    std::string_view getName() const override { return "ObjectionInNonRunPhase"; }
    std::string_view getDescription() const override {
        return "Disallows calling raise_objection() or drop_objection() inside non-run function phases (e.g. build_phase, connect_phase). "
               "Objections are strictly reserved for runtime phases.";
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
            for (const model::Method& sub : cls.methods) {
                if (sub.isExtern || sub.phase != model::PhaseKind::Function) continue;
                for (const model::Call& call : sub.calls) {
                    if (call.name != "raise_objection" && call.name != "drop_objection") continue;
                    diags.addViolation(
                        "UVM-PHS-009",
                        "ObjectionInNonRunPhase",
                        "Objection method '." + call.name + "()' called inside function phase '" + sub.name + "'. "
                        "In UVM methodology (IEEE 1800.2 § 9.2), phase objections control progression and completion strictly "
                        "during runtime phases ('run_phase' and scheduled sub-phases). Function phases execute synchronously in "
                        "zero simulation time; raising or dropping objections in function phases is prohibited and corrupts UVM objection counting.",
                        core::Severity::Error,
                        model::toSlang(call.loc)
                    );
                }
            }
        }
    }
};

} // namespace uvmcheck::rules::uvm

namespace uvmcheck::rules {
std::unique_ptr<Rule> createObjectionInNonRunPhase() {
    return std::make_unique<uvm::ObjectionInNonRunPhase>();
}
}

UVMCHECK_REGISTER_RULE(uvmcheck::rules::createObjectionInNonRunPhase)
