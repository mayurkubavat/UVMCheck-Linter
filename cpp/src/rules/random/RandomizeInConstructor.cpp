#include "uvmcheck/rules/Rule.h"
#include "uvmcheck/rules/RuleRegistry.h"
#include "uvmcheck/model/SlangModel.h"

namespace uvmcheck::rules::random {

class RandomizeInConstructor : public Rule {
public:
    std::string_view getId() const override { return "SV-RND-018"; }
    std::string_view getName() const override { return "RandomizeInConstructor"; }
    std::string_view getDescription() const override {
        return "Disallows calling randomize() inside class constructors (new()). "
               "Randomizing during construction breaks derived-class constraint inheritance and factory overrides.";
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
            for (const model::Method& sub : cls.methods) {
                if (sub.isExtern || sub.name != "new" || sub.isBuiltIn) continue;
                for (const model::Call& call : sub.calls) {
                    if (call.name != "randomize") continue;
                    diags.addViolation(
                        "SV-RND-018",
                        "RandomizeInConstructor",
                        "Calling 'randomize()' inside class constructor 'new()' of class '" + cls.name +
                        "'. In SystemVerilog OOP, calling randomize() during object construction executes before derived class members, "
                        "constraints, and factory overrides are fully constructed and initialized. "
                        "Stimulus randomization should always occur explicitly on fully constructed instances (e.g. 'if (!item.randomize()) ...').",
                        core::Severity::Warning,
                        model::toSlang(call.loc)
                    );
                }
            }
        }
    }
};

} // namespace uvmcheck::rules::random

namespace uvmcheck::rules {
std::unique_ptr<Rule> createRandomizeInConstructor() {
    return std::make_unique<random::RandomizeInConstructor>();
}
}

UVMCHECK_REGISTER_RULE(uvmcheck::rules::createRandomizeInConstructor)
