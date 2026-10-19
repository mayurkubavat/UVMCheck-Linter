#include "uvmcheck/rules/Rule.h"
#include "uvmcheck/rules/RuleRegistry.h"
#include "uvmcheck/uvm/UvmModel.h"

namespace uvmcheck::rules::style {

class UvmComponentNamingConvention : public Rule {
public:
    std::string_view getId() const override { return "SV-NAM-001"; }
    std::string_view getName() const override { return "UvmComponentNamingConvention"; }
    std::string_view getDescription() const override {
        return "Enforces standard naming conventions for UVM components: classes derived from uvm_driver must end "
               "with '_driver', uvm_monitor with '_monitor', uvm_agent with '_agent', uvm_env with '_env', and uvm_test with '_test'.";
    }
    std::string_view getCategory() const override { return "Naming"; }
    core::Severity getDefaultSeverity() const override { return core::Severity::Warning; }

    void run(slang::ast::Compilation&,
             const uvmcheck::uvm::UvmModel& uvmModel,
             core::DiagnosticEngine& diags) override {
        for (const auto* classType : uvmModel.getAllClasses()) {
            if (!classType) continue;
            std::string_view name = classType->name;
            if (name.starts_with("uvm_") || name.empty()) continue;

            struct SuffixCheck {
                std::string_view baseClass;
                std::string_view requiredSuffix;
                std::string_view role;
            };

            static const SuffixCheck checks[] = {
                { "uvm_driver", "_driver", "driver" },
                { "uvm_monitor", "_monitor", "monitor" },
                { "uvm_agent", "_agent", "agent" },
                { "uvm_env", "_env", "environment" },
                { "uvm_test", "_test", "test" }
            };

            for (const auto& check : checks) {
                if (uvmModel.isDerivedFrom(*classType, check.baseClass)) {
                    if (!name.ends_with(check.requiredSuffix)) {
                        diags.addViolation(
                            "SV-NAM-001",
                            "UvmComponentNamingConvention",
                            "UVM " + std::string(check.role) + " class '" + std::string(name) +
                            "' derived from '" + std::string(check.baseClass) +
                            "' does not end with required suffix '" + std::string(check.requiredSuffix) +
                            "'. Following uniform naming conventions is essential for VIP maintainability and automated testbench reflection.",
                            core::Severity::Warning,
                            classType->location
                        );
                    }
                    break;
                }
            }
        }
    }
};

} // namespace uvmcheck::rules::style

namespace uvmcheck::rules {
std::unique_ptr<Rule> createUvmComponentNamingConvention() {
    return std::make_unique<style::UvmComponentNamingConvention>();
}
}
