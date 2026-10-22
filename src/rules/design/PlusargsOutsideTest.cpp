#include "uvmcheck/rules/Rule.h"
#include "uvmcheck/rules/RuleRegistry.h"
#include "uvmcheck/rules/SweSupport.h"

namespace uvmcheck::rules::design {

using namespace slang::ast;

namespace {

bool isPlusargRead(const CallExpression& call) {
    std::string_view n = call.getSubroutineName();
    if (call.isSystemCall()) return n == "$test$plusargs" || n == "$value$plusargs";
    const auto* sub = std::get_if<const SubroutineSymbol*>(&call.subroutine);
    if (!sub || !*sub || !swe::isLibrarySymbol(**sub)) return false;
    return n == "get_arg_value" || n == "get_arg_values" || n == "get_arg_matches" || n == "get_plusargs";
}

} // namespace

class PlusargsOutsideTest : public Rule {
public:
    std::string_view getId() const override { return "UVM-CFG-006"; }
    std::string_view getName() const override { return "PlusargsOutsideTest"; }
    std::string_view getDescription() const override {
        return "Flags $test$plusargs, $value$plusargs and uvm_cmdline_processor reads in classes other "
               "than tests and config objects. Reading the command line deep inside components is "
               "hidden global configuration.";
    }
    std::string_view getCategory() const override { return "UVM"; }
    core::Severity getDefaultSeverity() const override { return core::Severity::Warning; }

    void run(Compilation&, const uvm::UvmModel& model, core::DiagnosticEngine& diags) override {
        for (const auto* cls : swe::userClasses(model)) {
            swe::Role role = swe::classRole(*cls);
            if (role == swe::Role::Test || role == swe::Role::Config) continue;
            for (const auto* m : swe::classMethods(*cls)) {
                swe::forEachCall(m->getBody(), [&](const CallExpression& call) {
                    if (!isPlusargRead(call)) return;
                    diags.addViolation(
                        "UVM-CFG-006", "PlusargsOutsideTest",
                        std::string(swe::roleName(role)) + " '" + std::string(cls->name) +
                            "' reads the command line with " + std::string(call.getSubroutineName()) +
                            "() in " + std::string(m->name) + "(). Hidden switches make runs hard to "
                            "reproduce and the class hard to reuse; read plusargs once in the test or a "
                            "config object and pass the values down through the config.",
                        core::Severity::Warning, swe::locOf(call));
                });
            }
        }
    }
};

} // namespace uvmcheck::rules::design

namespace uvmcheck::rules {
std::unique_ptr<Rule> createPlusargsOutsideTest() {
    return std::make_unique<design::PlusargsOutsideTest>();
}
}
