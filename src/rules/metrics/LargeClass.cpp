#include "uvmcheck/rules/Rule.h"
#include "uvmcheck/rules/RuleRegistry.h"
#include "uvmcheck/rules/CodeMetrics.h"
#include "uvmcheck/rules/SweSupport.h"

namespace uvmcheck::rules::metrics {

using namespace slang::ast;

class LargeClass : public Rule {
public:
    std::string_view getId() const override { return "SV-MET-006"; }
    std::string_view getName() const override { return "LargeClass"; }
    std::string_view getDescription() const override {
        return "Flags classes with more than 30 properties or 30 methods; config objects and sequence "
               "items may have 80 properties, register models are exempt (PMD TooManyFields and "
               "TooManyMethods, Pylint R0902/R0904).";
    }
    std::string_view getCategory() const override { return "Maintainability"; }
    core::Severity getDefaultSeverity() const override { return core::Severity::Info; }

    void run(Compilation& compilation, const uvm::UvmModel& model, core::DiagnosticEngine& diags) override {
        const int64_t maxFields = swe::ruleOption("SV-MET-006", "max_fields", 30);
        const int64_t maxDataFields = swe::ruleOption("SV-MET-006", "max_data_fields", 80);
        const int64_t maxMethods = swe::ruleOption("SV-MET-006", "max_methods", 30);
        for (const auto& cm : classMetrics(compilation, model)) {
            swe::Role role = swe::classRole(*cm.cls);
            if (role == swe::Role::RegModel) continue;
            bool data = role == swe::Role::Config || role == swe::Role::Item;
            int64_t fieldLimit = data ? maxDataFields : maxFields;
            std::string what;
            if (cm.fields > fieldLimit)
                what = std::to_string(cm.fields) + " properties (limit " + std::to_string(fieldLimit) + ")";
            if (cm.methods > maxMethods) {
                if (!what.empty()) what += " and ";
                what += std::to_string(cm.methods) + " methods (limit " + std::to_string(maxMethods) + ")";
            }
            if (what.empty()) continue;
            diags.addViolation(
                "SV-MET-006", "LargeClass",
                std::string(swe::roleName(role)) + " '" + std::string(cm.cls->name) + "' has " + what +
                    ". Large classes are hard to review and usually hold several responsibilities; split "
                    "out sub-configs, helpers or separate components.",
                core::Severity::Info, cm.cls->location);
        }
    }
};

} // namespace uvmcheck::rules::metrics

namespace uvmcheck::rules {
std::unique_ptr<Rule> createLargeClass() {
    return std::make_unique<metrics::LargeClass>();
}
}
