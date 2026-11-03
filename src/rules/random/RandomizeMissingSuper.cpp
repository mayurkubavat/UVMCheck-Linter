#include "uvmcheck/rules/Rule.h"
#include "uvmcheck/rules/RuleRegistry.h"
#include "uvmcheck/model/SlangModel.h"

namespace uvmcheck::rules::random {

class RandomizeMissingSuper : public Rule {
public:
    std::string_view getId() const override { return "SV-RND-003"; }
    std::string_view getName() const override { return "RandomizeMissingSuper"; }
    std::string_view getDescription() const override {
        return "Overridden pre_randomize() and post_randomize() callback methods must invoke super.pre_randomize() "
               "or super.post_randomize() to prevent bypassing base-class transaction setup and validation routines.";
    }
    std::string_view getCategory() const override { return "Randomization"; }
    core::Severity getDefaultSeverity() const override { return core::Severity::Warning; }

    // Reads only uvmcheck::model, not slang's AST.
    void run(slang::ast::Compilation&,
             const uvmcheck::uvm::UvmModel& uvmModel,
             core::DiagnosticEngine& diags) override {
        const model::Model& design = uvmModel.facade();
        for (const model::Class& cls : design.classes()) {
            if (cls.isParameterized) continue;
            if (cls.base == model::NoClass) continue;
            for (const model::Method& sub : cls.methods) {
                if (sub.isExtern || sub.isBuiltIn) continue;
                if (sub.name != "pre_randomize" && sub.name != "post_randomize") continue;
                if (!hasUserCallbackAbove(design, cls, sub.name)) continue;
                bool callsSuper = false;
                for (const model::Call& call : sub.calls) {
                    if (!call.isSystem && call.name == sub.name && call.viaSuper)
                        callsSuper = true;
                }
                if (callsSuper) continue;
                diags.addViolation(
                    "SV-RND-003",
                    "RandomizeMissingSuper",
                    "Overridden randomization callback '" + sub.name + "' in class '" +
                    cls.name + "' does not invoke 'super." + sub.name + "()'. "
                    "Unlike UVM phases, SV pre/post_randomize callbacks do not auto-chain up inheritance; "
                    "omitting super bypasses base-class payload, CRC, and sequence updates.",
                    core::Severity::Warning,
                    model::toSlang(sub.loc)
                );
            }
        }
    }

private:
    static bool hasUserCallbackAbove(const model::Model& design, const model::Class& cls, std::string_view name) {
        for (model::ClassId a : design.ancestors(design.idOf(cls))) {
            const model::Method* found = design.cls(a).findMethod(name);
            if (found && !found->isBuiltIn) return true;
        }
        return false;
    }
};

} // namespace uvmcheck::rules::random

namespace uvmcheck::rules {
std::unique_ptr<Rule> createRandomizeMissingSuper() {
    return std::make_unique<random::RandomizeMissingSuper>();
}
}

UVMCHECK_REGISTER_RULE(uvmcheck::rules::createRandomizeMissingSuper)
