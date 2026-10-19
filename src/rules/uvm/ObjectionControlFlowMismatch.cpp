#include "uvmcheck/rules/Rule.h"
#include "uvmcheck/rules/RuleRegistry.h"
#include "uvmcheck/uvm/ObjectionControlFlow.h"

namespace uvmcheck::rules::uvm {

class ObjectionControlFlowMismatch : public Rule {
public:
    std::string_view getId() const override { return "UVM-OBJ-003"; }
    std::string_view getName() const override { return "ObjectionControlFlowMismatch"; }
    std::string_view getDescription() const override {
        return "Performs control-flow graph (CFG) analysis on UVM phase and task routines to verify that "
               "every execution path containing raise_objection() is strictly guaranteed to reach a matching "
               "drop_objection() without early returns or asymmetric conditional branching.";
    }
    std::string_view getCategory() const override { return "UVM"; }
    core::Severity getDefaultSeverity() const override { return core::Severity::Error; }

    void run(slang::ast::Compilation& /*compilation*/,
             const uvmcheck::uvm::UvmModel& uvmModel,
             core::DiagnosticEngine& diags) override {
        for (const auto* cls : uvmModel.getAllClasses()) {
            if (!cls) continue;
            bool isComp = uvmModel.isUvmComponent(*cls);
            bool isSeq = uvmModel.isUvmSequence(*cls);

            if (!isComp && !isSeq) continue;

            for (const auto& member : cls->members()) {
                if (member.kind != slang::ast::SymbolKind::Subroutine) continue;

                const auto& sub = member.as<slang::ast::SubroutineSymbol>();
                if (sub.flags.has(slang::ast::MethodFlags::BuiltIn)) continue;

                auto violations = uvmcheck::uvm::ObjectionControlFlowAnalyzer::analyzeSubroutine(sub, cls->name);
                for (const auto& v : violations) {
                    diags.addViolation(v.ruleId, v.ruleName, v.message, v.severity, v.location);
                }
            }
        }
    }
};

} // namespace uvmcheck::rules::uvm

namespace uvmcheck::rules {
std::unique_ptr<Rule> createObjectionControlFlowMismatch() {
    return std::make_unique<uvm::ObjectionControlFlowMismatch>();
}
}
