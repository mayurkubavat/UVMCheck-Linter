#include "uvmcheck/rules/Rule.h"
#include "uvmcheck/rules/RuleRegistry.h"
#include "slang/ast/symbols/ClassSymbols.h"
#include "slang/ast/symbols/SubroutineSymbols.h"

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

    void run(slang::ast::Compilation&,
             const uvmcheck::uvm::UvmModel& uvmModel,
             core::DiagnosticEngine& diags) override {
        for (const auto* cls : uvmModel.getAllClasses()) {
            if (!cls || !uvmModel.isUvmSequence(*cls)) continue;

            for (const auto& member : cls->members()) {
                if (member.kind == slang::ast::SymbolKind::Subroutine) {
                    const auto& sub = member.as<slang::ast::SubroutineSymbol>();
                    if (sub.name == "pre_body" || sub.name == "post_body") {
                        diags.addViolation(
                            "UVM-SEQ-002",
                            "UvmSequencePrePostBodyMisuse",
                            "Sequence '" + std::string(cls->name) + "' overrides lifecycle callback '" + std::string(sub.name) + "()'. "
                            "In UVM sequence architecture, pre_body() and post_body() are explicitly skipped when a sequence is executed as a child sequence "
                            "via `uvm_do or `uvm_do_with. Critical setup or cleanup logic will silently fail to execute when nested. "
                            "Migrate logic to pre_start() / post_start() (which execute in all modes) or body().",
                            core::Severity::Warning,
                            sub.location
                        );
                    }
                }
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
