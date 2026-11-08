#include "uvmcheck/rules/Rule.h"
#include "uvmcheck/rules/RuleRegistry.h"
#include "uvmcheck/core/ConfigManager.h"
#include "uvmcheck/style/NamingEngine.h"

#include <algorithm>

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

    std::vector<SubRule> getSubRules() const override {
        uvmcheck::style::NamingEngine defaults;
        defaults.loadDefaultRules();
        std::vector<SubRule> subs;
        for (const auto& [kind, spec] : defaults.getRules()) {
            subs.push_back({spec.ruleId, kind + "Naming", spec.customMessage, spec.severity});
        }
        std::sort(subs.begin(), subs.end(), [](const auto& a, const auto& b) { return a.id < b.id; });
        return subs;
    }

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

    std::vector<SubRule> getSubRules() const override {
        using S = core::Severity;
        return {
            {"SV-STY-101", "NoTabs", "Literal tab characters are not allowed (style.allow_tabs).", S::Warning},
            {"SV-STY-102", "NoTrailingWhitespace", "Lines must not end in whitespace (style.allow_trailing_whitespace).", S::Warning},
            {"SV-STY-103", "MaxLineLength", "Lines must not exceed style.max_line_length characters.", S::Warning},
            {"SV-STY-104", "MatchingEndLabel", "Blocks, routines and classes carry an end label that matches their name (style.mandatory_matching_end_label).", S::Warning},
            {"SV-STY-105", "FileMatchesClassName", "A file's base name matches the top-level class it defines.", S::Warning},
            {"SV-STY-106", "FileHeader", "Files start with a header matching style.file_header_pattern (style.require_file_header).", S::Warning},
        };
    }

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

UVMCHECK_REGISTER_RULE(uvmcheck::rules::createConfigurableNamingEngine)
UVMCHECK_REGISTER_RULE(uvmcheck::rules::createConfigurableStyleEngine)
