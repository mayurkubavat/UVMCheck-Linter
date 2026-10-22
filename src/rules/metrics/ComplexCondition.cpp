#include "uvmcheck/rules/Rule.h"
#include "uvmcheck/rules/RuleRegistry.h"
#include "uvmcheck/rules/CodeMetrics.h"
#include "uvmcheck/rules/SweSupport.h"

namespace uvmcheck::rules::metrics {

using namespace slang::ast;

class ComplexCondition : public Rule {
public:
    std::string_view getId() const override { return "SV-MET-005"; }
    std::string_view getName() const override { return "ComplexCondition"; }
    std::string_view getDescription() const override {
        return "Flags conditions with more && / || operators than the threshold (default 5; Pylint R0916, "
               "Checkstyle BooleanExpressionComplexity).";
    }
    std::string_view getCategory() const override { return "Maintainability"; }
    core::Severity getDefaultSeverity() const override { return core::Severity::Warning; }

    void run(Compilation& compilation, const uvm::UvmModel& model, core::DiagnosticEngine& diags) override {
        const int64_t limit = swe::ruleOption("SV-MET-005", "max_bool_ops", 5);
        for (const auto* sub : swe::allUserRoutines(compilation, model)) {
            auto m = computeRoutineMetrics(*sub);
            if (m.maxBoolOps <= limit) continue;
            diags.addViolation(
                "SV-MET-005", "ComplexCondition",
                "This condition in '" + std::string(sub->name) + "' combines " + std::to_string(m.maxBoolOps) +
                    " && / || operators (limit " + std::to_string(limit) + "). Name the parts with local "
                    "bits or a helper function so the check can be reviewed.",
                core::Severity::Warning, m.mostComplexCondition.valid() ? m.mostComplexCondition : sub->location);
        }
    }
};

} // namespace uvmcheck::rules::metrics

namespace uvmcheck::rules {
std::unique_ptr<Rule> createComplexCondition() {
    return std::make_unique<metrics::ComplexCondition>();
}
}
