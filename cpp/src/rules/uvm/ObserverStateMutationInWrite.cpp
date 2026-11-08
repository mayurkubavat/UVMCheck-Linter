#include "uvmcheck/rules/Rule.h"
#include "uvmcheck/rules/RuleRegistry.h"
#include "uvmcheck/model/SlangModel.h"

namespace uvmcheck::rules::uvm {

class ObserverStateMutationInWrite : public Rule {
public:
    std::string_view getId() const override { return "UVM-OBS-001"; }
    std::string_view getName() const override { return "ObserverStateMutationInWrite"; }
    std::string_view getDescription() const override {
        return "Mutating the observed transaction object inside a subscriber's write() method corrupts state "
               "for other concurrent subscribers in the Observer pattern. Clone the transaction before mutating.";
    }
    std::string_view getCategory() const override { return "Architecture"; }
    core::Severity getDefaultSeverity() const override { return core::Severity::Error; }

    // Reads only uvmcheck::model, not slang's AST.
    void run(slang::ast::Compilation&,
             const uvmcheck::uvm::UvmModel& uvmModel,
             core::DiagnosticEngine& diags) override {
        const model::Model& design = uvmModel.facade();
        for (const model::Class& cls : design.classes()) {
            if (cls.isParameterized) continue;
            for (const model::Method& sub : cls.methods) {
                if (sub.isExtern || sub.name != "write" || sub.isTask) continue;
                if (sub.arguments.empty() || sub.arguments[0].typeClass == model::NoClass) continue;
                const std::string& arg = sub.arguments[0].name;
                for (const model::Assignment& assign : sub.assignments) {
                    if (assign.rootKind != model::RootKind::Argument || assign.rootName != arg) continue;
                    diags.addViolation(
                        "UVM-OBS-001",
                        "ObserverStateMutationInWrite",
                        "In-place mutation of observed transaction argument '" + arg +
                        "' in 'write()' method of class '" + cls.name + "'. "
                        "In the UVM Observer pattern, the same transaction handle is broadcast to multiple subscribers "
                        "(scoreboards, functional coverage, checkers). Mutating transaction fields directly corrupts data "
                        "for all downstream observers. Clone the transaction ($cast(cloned_t, " + arg + ".clone())) before mutating.",
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
std::unique_ptr<Rule> createObserverStateMutationInWrite() {
    return std::make_unique<uvm::ObserverStateMutationInWrite>();
}
}

UVMCHECK_REGISTER_RULE(uvmcheck::rules::createObserverStateMutationInWrite)
