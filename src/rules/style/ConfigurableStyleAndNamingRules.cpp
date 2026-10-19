#include "uvmcheck/rules/Rule.h"
#include "uvmcheck/rules/RuleRegistry.h"
#include "uvmcheck/core/ConfigManager.h"

namespace uvmcheck::rules::style {

class ConfigurableNamingEngineRule : public Rule {
public:
    std::string_view getId() const override { return "SV-NAM-100"; }
    std::string_view getName() const override { return "ConfigurableNamingEngine"; }
    std::string_view getDescription() const override {
        return "Parameterizable naming convention engine validating regex, prefix, suffix, and casing conventions across packages, interfaces, UVM classes, covergroups, and constraints.";
    }
    std::string_view getCategory() const override { return "Style"; }
    core::Severity getDefaultSeverity() const override { return core::Severity::Warning; }

    void run(slang::ast::Compilation& compilation,
             const uvmcheck::uvm::UvmModel& uvmModel,
             core::DiagnosticEngine& diags) override {
        core::ConfigManager::instance().getNamingEngine().run(compilation, uvmModel, diags);
    }
};

class ConfigurableStyleEngineRule : public Rule {
public:
    std::string_view getId() const override { return "SV-STY-100"; }
    std::string_view getName() const override { return "ConfigurableStyleEngine"; }
    std::string_view getDescription() const override {
        return "Parameterizable code style engine enforcing tab prohibitions, trailing whitespace limits, maximum line lengths, matching block end labels, and file naming.";
    }
    std::string_view getCategory() const override { return "Style"; }
    core::Severity getDefaultSeverity() const override { return core::Severity::Warning; }

    void run(slang::ast::Compilation& compilation,
             const uvmcheck::uvm::UvmModel& uvmModel,
             core::DiagnosticEngine& diags) override {
        core::ConfigManager::instance().getStyleEngine().run(compilation, uvmModel, diags);
    }
};

} // namespace uvmcheck::rules::style

namespace uvmcheck::rules {
std::unique_ptr<Rule> createConfigurableNamingEngine() {
    return std::make_unique<style::ConfigurableNamingEngineRule>();
}

std::unique_ptr<Rule> createConfigurableStyleEngine() {
    return std::make_unique<style::ConfigurableStyleEngineRule>();
}
} // namespace uvmcheck::rules
