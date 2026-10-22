#include "uvmcheck/rules/Rule.h"
#include "uvmcheck/rules/RuleRegistry.h"
#include "uvmcheck/rules/SweSupport.h"
#include "slang/ast/symbols/CompilationUnitSymbols.h"
#include "slang/ast/symbols/VariableSymbols.h"

namespace uvmcheck::rules::design {

using namespace slang::ast;

class MutablePackageState : public Rule {
public:
    std::string_view getId() const override { return "SV-DSN-008"; }
    std::string_view getName() const override { return "MutablePackageState"; }
    std::string_view getDescription() const override {
        return "Flags non-const variables declared in packages. They are shared by every env instance "
               "and test in a run (clang-tidy cppcoreguidelines-avoid-non-const-global-variables).";
    }
    std::string_view getCategory() const override { return "Architecture"; }
    core::Severity getDefaultSeverity() const override { return core::Severity::Warning; }

    void run(Compilation& compilation, const uvm::UvmModel&, core::DiagnosticEngine& diags) override {
        for (const auto* pkg : compilation.getPackages()) {
            if (!pkg || pkg->name == "std" || swe::isLibrarySymbol(*pkg)) continue;
            for (const auto& member : pkg->members()) {
                if (member.kind != SymbolKind::Variable) continue;
                const auto& var = member.as<VariableSymbol>();
                if (var.flags.has(VariableFlags::Const) || var.flags.has(VariableFlags::CompilerGenerated))
                    continue;
                diags.addViolation(
                    "SV-DSN-008", "MutablePackageState",
                    "Package variable '" + std::string(pkg->name) + "::" + std::string(var.name) +
                        "' is global mutable state shared by every env instance and test in the run. "
                        "Move it into a config object or component, or make it const.",
                    core::Severity::Warning, var.location);
            }
        }
    }
};

} // namespace uvmcheck::rules::design

namespace uvmcheck::rules {
std::unique_ptr<Rule> createMutablePackageState() {
    return std::make_unique<design::MutablePackageState>();
}
}
