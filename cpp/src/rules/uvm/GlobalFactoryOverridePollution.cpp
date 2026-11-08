// UVM-FAC-005: global factory type overrides belong in tests.
//
// A type override changes what every create() of that type returns, for the whole
// simulation. Called from a reusable component (an agent, a VIP's env, a scoreboard) or a
// sequence, it silently changes every other test and VIP that uses the type. Tests own
// that decision; reusable code uses instance overrides scoped to its own sub-tree.
//
// Option: allow_in_env = 1 also accepts overrides in uvm_env classes, for projects whose
// top env acts as the test's configuration layer.

#include "uvmcheck/rules/Rule.h"
#include "uvmcheck/rules/RuleRegistry.h"
#include "uvmcheck/rules/SweSupport.h"
#include "../methodology/MethodologySupport.h"
#include "slang/ast/expressions/CallExpression.h"
#include "slang/ast/symbols/ClassSymbols.h"
#include "slang/ast/symbols/SubroutineSymbols.h"

namespace uvmcheck::rules::uvm {

using namespace slang::ast;

namespace {

/// True for calls that install a global (type-wide) factory override.
bool isGlobalTypeOverride(const CallExpression& call) {
    std::string_view owner = meth::declaringClassName(call);
    std::string_view n = call.getSubroutineName();
    if (owner == "uvm_component")
        return n == "set_type_override" || n == "set_type_override_by_type";
    if (owner == "uvm_factory" || owner == "uvm_default_factory")
        return n == "set_type_override_by_type" || n == "set_type_override_by_name";
    if (owner == "uvm_component_registry" || owner == "uvm_object_registry" ||
        owner == "uvm_abstract_component_registry" || owner == "uvm_abstract_object_registry")
        return n == "set_type_override";
    return false;
}

} // namespace

class GlobalFactoryOverridePollution : public Rule {
public:
    std::string_view getId() const override { return "UVM-FAC-005"; }
    std::string_view getName() const override { return "GlobalFactoryOverridePollution"; }
    std::string_view getDescription() const override {
        return "Flags global factory type overrides (set_type_override, set_type_override_by_type, "
               "factory.set_type_override_by_name, T::type_id::set_type_override) outside uvm_test "
               "classes. They change the type for every test and VIP in the simulation; reusable "
               "components and sequences should use instance overrides scoped to their sub-tree.";
    }
    std::string_view getCategory() const override { return "UVM"; }
    core::Severity getDefaultSeverity() const override { return core::Severity::Warning; }

    // Queries the class model only; it does not walk the design.
    void run(Compilation&, const uvmcheck::uvm::UvmModel& model, core::DiagnosticEngine& diags) override {
        const bool allowInEnv = swe::ruleOption(getId(), "allow_in_env", 0) != 0;
        for (const auto* cls : swe::userClasses(model)) {
            const swe::Role role = swe::classRole(*cls);
            if (role == swe::Role::Test || role == swe::Role::None) continue;
            if (role == swe::Role::Env && allowInEnv) continue;

            for (const auto* method : swe::classMethods(*cls)) {
                swe::forEachCall(method->getBody(), [&](const CallExpression& call) {
                    if (!isGlobalTypeOverride(call)) return;
                    diags.addViolation(
                        "UVM-FAC-005", "GlobalFactoryOverridePollution",
                        std::string(call.getSubroutineName()) + "() in " +
                            swe::roleName(role) + " '" + std::string(cls->name) +
                            "' installs a global type override, which changes this type for every "
                            "test and VIP in the simulation. Leave type overrides to the test, or use "
                            "an instance override (set_inst_override_by_type) scoped to this "
                            "component's sub-tree.",
                        core::Severity::Warning, swe::locOf(call));
                });
            }
        }
    }
};

} // namespace uvmcheck::rules::uvm

namespace uvmcheck::rules {
std::unique_ptr<Rule> createGlobalFactoryOverridePollution() {
    return std::make_unique<uvm::GlobalFactoryOverridePollution>();
}
} // namespace uvmcheck::rules

UVMCHECK_REGISTER_RULE(uvmcheck::rules::createGlobalFactoryOverridePollution)
