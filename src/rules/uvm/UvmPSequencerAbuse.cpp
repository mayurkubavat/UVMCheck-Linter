#include "uvmcheck/rules/Rule.h"
#include "uvmcheck/rules/RuleRegistry.h"
#include "slang/ast/symbols/ClassSymbols.h"

namespace uvmcheck::rules::uvm {

class UvmPSequencerAbuse : public Rule {
public:
    std::string_view getId() const override { return "UVM-SEQ-003"; }
    std::string_view getName() const override { return "UvmPSequencerAbuse"; }
    std::string_view getDescription() const override {
        return "Using `uvm_declare_p_sequencer or accessing p_sequencer violates the Law of Demeter by tightly coupling "
               "the sequence data tier to physical sequencer component internals. Pass configuration down through sequence fields or uvm_config_db.";
    }
    std::string_view getCategory() const override { return "Architecture"; }
    core::Severity getDefaultSeverity() const override { return core::Severity::Warning; }

    void run(slang::ast::Compilation&,
             const uvmcheck::uvm::UvmModel& uvmModel,
             core::DiagnosticEngine& diags) override {
        for (const auto* cls : uvmModel.getAllClasses()) {
            if (!cls || !uvmModel.isUvmSequence(*cls)) continue;

            for (const auto& member : cls->members()) {
                if (member.kind == slang::ast::SymbolKind::ClassProperty && member.name == "p_sequencer") {
                    diags.addViolation(
                        "UVM-SEQ-003",
                        "UvmPSequencerAbuse",
                        "Sequence '" + std::string(cls->name) + "' declares 'p_sequencer' via `uvm_declare_p_sequencer. "
                        "This violates the Law of Demeter and architectural layering by tightly binding sequence scenario generation "
                        "to physical testbench agent/sequencer topology. If sequencer topology or naming changes, sequences break. "
                        "Decouple sequences by passing configuration parameters down through sequence properties or uvm_config_db.",
                        core::Severity::Warning,
                        member.location
                    );
                }
            }
        }
    }
};

} // namespace uvmcheck::rules::uvm

namespace uvmcheck::rules {
std::unique_ptr<Rule> createUvmPSequencerAbuse() {
    return std::make_unique<uvm::UvmPSequencerAbuse>();
}
}
