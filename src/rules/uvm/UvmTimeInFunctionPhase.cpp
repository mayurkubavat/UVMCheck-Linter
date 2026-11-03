#include "uvmcheck/rules/Rule.h"
#include "uvmcheck/rules/RuleRegistry.h"
#include "uvmcheck/model/SlangModel.h"

namespace uvmcheck::rules::uvm {

class UvmTimeInFunctionPhase : public Rule {
public:
    std::string_view getId() const override { return "UVM-PHS-002"; }
    std::string_view getName() const override { return "UvmTimeInFunctionPhase"; }
    std::string_view getDescription() const override {
        return "Function-based UVM phases (e.g., build_phase, connect_phase) must not consume simulation "
               "time using delays (#), event controls (@), or wait statements.";
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
            if (!model::isComponent(cls.uvm)) continue;
            for (const model::Method& sub : cls.methods) {
                if (sub.isExtern || sub.phase != model::PhaseKind::Function || sub.timingControls.empty()) continue;
                diags.addViolation(
                    "UVM-PHS-002",
                    "UvmTimeInFunctionPhase",
                    "Time-consuming statement detected inside function phase '" +
                    sub.name + "' in class '" + cls.name + "'.",
                    core::Severity::Error,
                    model::toSlang(sub.timingControls.back()) // the last one, as before the port
                );
            }
        }
    }
};

} // namespace uvmcheck::rules::uvm

namespace uvmcheck::rules {
std::unique_ptr<Rule> createUvmTimeInFunctionPhase() {
    return std::make_unique<uvm::UvmTimeInFunctionPhase>();
}
}

UVMCHECK_REGISTER_RULE(uvmcheck::rules::createUvmTimeInFunctionPhase)
