#include "uvmcheck/rules/Rule.h"
#include "uvmcheck/rules/RuleRegistry.h"
#include "uvmcheck/rules/SweSupport.h"

namespace uvmcheck::rules::random {

using namespace slang::ast;

class ConstantReseed : public Rule {
public:
    std::string_view getId() const override { return "SV-TST-001"; }
    std::string_view getName() const override { return "ConstantReseed"; }
    std::string_view getDescription() const override {
        return "Flags srandom() or set_randstate() with a constant in components, sequences and other "
               "non-test classes; every seed of the regression then runs the same stimulus. Advisory: "
               "off by default.";
    }
    std::string_view getCategory() const override { return "Advisory"; }
    core::Severity getDefaultSeverity() const override { return core::Severity::Info; }

    void run(Compilation&, const uvm::UvmModel& model, core::DiagnosticEngine& diags) override {
        for (const auto* cls : swe::userClasses(model)) {
            if (swe::classRole(*cls) == swe::Role::Test) continue;
            for (const auto* m : swe::classMethods(*cls)) {
                swe::forEachCall(m->getBody(), [&](const CallExpression& call) {
                    std::string_view n = call.getSubroutineName();
                    if (n != "srandom" && n != "set_randstate") return;
                    auto args = call.arguments();
                    if (args.empty() || !swe::isConstantExpr(*args.back())) return;
                    diags.addViolation(
                        "SV-TST-001", "ConstantReseed",
                        "'" + swe::exprText(call) + "' in " + std::string(cls->name) + "::" +
                            std::string(m->name) + " reseeds with a constant, so this object produces the "
                            "same random values for every simulation seed. Remove it, or derive the seed "
                            "from the global seed (for example with get_full_name()).",
                        core::Severity::Info, swe::locOf(call));
                });
            }
        }
    }
};

} // namespace uvmcheck::rules::random

namespace uvmcheck::rules {
std::unique_ptr<Rule> createConstantReseed() {
    return std::make_unique<random::ConstantReseed>();
}
}
