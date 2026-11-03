#include "uvmcheck/rules/Rule.h"
#include "uvmcheck/rules/RuleRegistry.h"
#include "uvmcheck/model/SlangModel.h"

namespace uvmcheck::rules::uvm {

class UvmSequencePrePostBodyMisuse : public Rule {
public:
    std::string_view getId() const override { return "UVM-SEQ-002"; }
    std::string_view getName() const override { return "UvmSequencePrePostBodyMisuse"; }
    std::string_view getDescription() const override {
        return "pre_body() and post_body() callbacks in uvm_sequence are explicitly bypassed when the sequence is "
               "executed as a child sequence via `uvm_do or `uvm_do_with. Use pre_start() / post_start() or body() instead.";
    }
    std::string_view getCategory() const override { return "Architecture"; }
    core::Severity getDefaultSeverity() const override { return core::Severity::Warning; }

    // Reads only uvmcheck::model, not slang's AST.
    void run(slang::ast::Compilation&,
             const uvmcheck::uvm::UvmModel& uvmModel,
             core::DiagnosticEngine& diags) override {
        const model::Model& design = uvmModel.facade();
        for (const model::Class& cls : design.classes()) {
            if (cls.uvm != model::UvmKind::Sequence) continue;
            for (const model::Method& m : cls.methods) {
                if (m.name != "pre_body" && m.name != "post_body") continue;
                diags.addViolation(
                    "UVM-SEQ-002",
                    "UvmSequencePrePostBodyMisuse",
                    "Sequence '" + cls.name + "' overrides lifecycle callback '" + m.name + "()'. "
                    "In UVM sequence architecture, pre_body() and post_body() are explicitly skipped when a sequence is executed as a child sequence "
                    "via `uvm_do or `uvm_do_with. Critical setup or cleanup logic will silently fail to execute when nested. "
                    "Migrate logic to pre_start() / post_start() (which execute in all modes) or body().",
                    core::Severity::Warning,
                    model::toSlang(m.loc)
                );
            }
        }
    }
};

} // namespace uvmcheck::rules::uvm

namespace uvmcheck::rules {
std::unique_ptr<Rule> createUvmSequencePrePostBodyMisuse() {
    return std::make_unique<uvm::UvmSequencePrePostBodyMisuse>();
}
}

UVMCHECK_REGISTER_RULE(uvmcheck::rules::createUvmSequencePrePostBodyMisuse)
