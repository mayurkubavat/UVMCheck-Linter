#include "uvmcheck/rules/Rule.h"
#include "uvmcheck/rules/RuleRegistry.h"
#include "uvmcheck/model/SlangModel.h"

namespace uvmcheck::rules::random {

class GlobalRngInClass : public Rule {
public:
    std::string_view getId() const override { return "SV-RND-002"; }
    std::string_view getName() const override { return "GlobalRngInClass"; }
    std::string_view getDescription() const override {
        return "Calling $urandom or $random inside class components or sequences corrupts global thread random "
               "stability. Use std::randomize() or object-scoped randomization to preserve simulation repeatability.";
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
                if (sub.isExtern || sub.isBuiltIn) continue; // in-class bodies, as before the port
                for (const model::Call& call : sub.calls) {
                    if (call.name != "$urandom" && call.name != "$urandom_range" && call.name != "$random") continue;
                    diags.addViolation(
                        "SV-RND-002",
                        "GlobalRngInClass",
                        "Invoking global pseudo-random system function '" + call.name +
                        "' inside class '" + cls.name +
                        "'. Global RNG calls consume the process-level PRNG state instead of the object's hierarchical random stream, "
                        "destroying simulation seed repeatability and testbench reproducibility. Use 'std::randomize()' or class-scoped RNG.",
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
std::unique_ptr<Rule> createGlobalRngInClass() {
    return std::make_unique<random::GlobalRngInClass>();
}
}

UVMCHECK_REGISTER_RULE(uvmcheck::rules::createGlobalRngInClass)
