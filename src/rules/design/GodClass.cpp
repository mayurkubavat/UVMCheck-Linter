#include "uvmcheck/rules/Rule.h"
#include "uvmcheck/rules/RuleRegistry.h"
#include "uvmcheck/rules/CodeMetrics.h"
#include "uvmcheck/rules/SweSupport.h"

#include <cstdio>

namespace uvmcheck::rules::design {

using namespace slang::ast;

class GodClass : public Rule {
public:
    std::string_view getId() const override { return "SV-DSN-002"; }
    std::string_view getName() const override { return "GodClass"; }
    std::string_view getDescription() const override {
        return "Flags classes that are very complex (WMC >= 47), use many other classes' data "
               "(ATFD > 5) and have low cohesion (TCC < 1/3): PMD's GodClass detection strategy.";
    }
    std::string_view getCategory() const override { return "Maintainability"; }
    core::Severity getDefaultSeverity() const override { return core::Severity::Warning; }

    void run(Compilation& compilation, const uvm::UvmModel& model, core::DiagnosticEngine& diags) override {
        const int64_t wmcLimit = swe::ruleOption("SV-DSN-002", "wmc", 47);
        const int64_t atfdLimit = swe::ruleOption("SV-DSN-002", "atfd", 5);
        const int64_t tccPercent = swe::ruleOption("SV-DSN-002", "tcc_percent", 33);
        for (const auto& cm : metrics::classMetrics(compilation, model)) {
            if (swe::classRole(*cm.cls) == swe::Role::RegModel) continue; // generated register models
            if (cm.wmc < wmcLimit || cm.atfd <= atfdLimit || cm.tcc * 100.0 >= static_cast<double>(tccPercent))
                continue;
            char tcc[16];
            std::snprintf(tcc, sizeof(tcc), "%.2f", cm.tcc);
            diags.addViolation(
                "SV-DSN-002", "GodClass",
                std::string(swe::roleName(swe::classRole(*cm.cls))) + " '" + std::string(cm.cls->name) +
                    "' is a god class: weighted method complexity " + std::to_string(cm.wmc) +
                    ", uses " + std::to_string(cm.atfd) + " fields of other classes, cohesion (TCC) " + tcc +
                    ". It does several jobs at once; split it, for example into predictor, comparator and "
                    "coverage parts.",
                core::Severity::Warning, cm.cls->location);
        }
    }
};

} // namespace uvmcheck::rules::design

namespace uvmcheck::rules {
std::unique_ptr<Rule> createGodClass() {
    return std::make_unique<design::GodClass>();
}
}
