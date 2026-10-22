#include "uvmcheck/rules/Rule.h"
#include "uvmcheck/rules/RuleRegistry.h"
#include "uvmcheck/rules/SweSupport.h"

namespace uvmcheck::rules::design {

using namespace slang::ast;

class StringHierarchyLookup : public Rule {
public:
    std::string_view getId() const override { return "UVM-ARC-004"; }
    std::string_view getName() const override { return "StringHierarchyLookup"; }
    std::string_view getDescription() const override {
        return "Flags uvm_root/uvm_component find(), find_all() and lookup() calls outside tests and "
               "end_of_elaboration_phase. Looking components up by path string is a service locator "
               "that hides dependencies and breaks silently when the hierarchy changes.";
    }
    std::string_view getCategory() const override { return "Architecture"; }
    core::Severity getDefaultSeverity() const override { return core::Severity::Warning; }

    void run(Compilation&, const uvm::UvmModel& model, core::DiagnosticEngine& diags) override {
        for (const auto* cls : swe::userClasses(model)) {
            if (swe::classRole(*cls) == swe::Role::Test) continue;
            for (const auto* m : swe::classMethods(*cls)) {
                if (m->name == "end_of_elaboration_phase") continue;
                swe::forEachCall(m->getBody(), [&](const CallExpression& call) {
                    if (call.isSystemCall()) return;
                    const auto* sub = std::get_if<const SubroutineSymbol*>(&call.subroutine);
                    if (!sub || !*sub || !swe::isLibrarySymbol(**sub)) return;
                    std::string_view n = (*sub)->name;
                    if (n != "find" && n != "find_all" && n != "lookup") return;
                    diags.addViolation(
                        "UVM-ARC-004", "StringHierarchyLookup",
                        std::string(swe::roleName(swe::classRole(*cls))) + " '" + std::string(cls->name) +
                            "' looks up components by path with '" + swe::exprText(call) + "' in " +
                            std::string(m->name) + "(). The dependency is invisible and the lookup returns "
                            "null as soon as the hierarchy is renamed. Get the handle through config_db, "
                            "a config object or a TLM connection.",
                        core::Severity::Warning, swe::locOf(call));
                });
            }
        }
    }
};

} // namespace uvmcheck::rules::design

namespace uvmcheck::rules {
std::unique_ptr<Rule> createStringHierarchyLookup() {
    return std::make_unique<design::StringHierarchyLookup>();
}
}
