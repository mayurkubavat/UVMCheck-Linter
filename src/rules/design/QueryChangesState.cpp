#include "uvmcheck/rules/Rule.h"
#include "uvmcheck/rules/RuleRegistry.h"
#include "uvmcheck/rules/SweSupport.h"

namespace uvmcheck::rules::design {

using namespace slang::ast;

class QueryChangesState : public Rule {
public:
    std::string_view getId() const override { return "SV-DSN-010"; }
    std::string_view getName() const override { return "QueryChangesState"; }
    std::string_view getDescription() const override {
        return "Flags functions named get_*, is_* or has_* that write class properties "
               "(command-query separation). get_next_* is allowed. Advisory: off by default.";
    }
    std::string_view getCategory() const override { return "Advisory"; }
    core::Severity getDefaultSeverity() const override { return core::Severity::Info; }

    void run(Compilation&, const uvm::UvmModel& model, core::DiagnosticEngine& diags) override {
        for (const auto* cls : swe::userClasses(model)) {
            for (const auto* m : swe::classMethods(*cls)) {
                std::string_view n = m->name;
                if (m->subroutineKind != SubroutineKind::Function) continue;
                if (!n.starts_with("get_") && !n.starts_with("is_") && !n.starts_with("has_")) continue;
                if (n.starts_with("get_next_")) continue;
                const Symbol* changed = nullptr;
                for (const auto* sym : swe::writtenSymbols(m->getBody())) {
                    if (sym->kind == SymbolKind::ClassProperty && (!changed || sym->name < changed->name))
                        changed = sym;
                }
                if (changed) {
                    const Symbol* sym = changed;
                    diags.addViolation(
                        "SV-DSN-010", "QueryChangesState",
                        "'" + std::string(cls->name) + "::" + std::string(n) + "' reads like a query but "
                            "changes '" + std::string(sym->name) + "'. Callers, checks and assertions "
                            "that call it twice see different results; split the update into its own "
                            "method or rename it.",
                        core::Severity::Info, m->location);
                }
            }
        }
    }
};

} // namespace uvmcheck::rules::design

namespace uvmcheck::rules {
std::unique_ptr<Rule> createQueryChangesState() {
    return std::make_unique<design::QueryChangesState>();
}
}
