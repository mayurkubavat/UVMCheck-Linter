#include "uvmcheck/rules/Rule.h"
#include "uvmcheck/rules/RuleRegistry.h"
#include "uvmcheck/rules/CodeMetrics.h"
#include "uvmcheck/rules/SweSupport.h"

namespace uvmcheck::rules::metrics {

using namespace slang::ast;

class DeepNesting : public Rule {
public:
    std::string_view getId() const override { return "SV-MET-004"; }
    std::string_view getName() const override { return "DeepNesting"; }
    std::string_view getDescription() const override {
        return "Flags routines whose control structures nest deeper than the threshold (default 5; Checkstyle "
               "NestedIfDepth, clang-tidy readability-function-size NestingThreshold).";
    }
    std::string_view getCategory() const override { return "Maintainability"; }
    core::Severity getDefaultSeverity() const override { return core::Severity::Warning; }

    void run(Compilation& compilation, const uvm::UvmModel& model, core::DiagnosticEngine& diags) override {
        const int64_t limit = swe::ruleOption("SV-MET-004", "max_depth", 5);
        for (const auto* sub : swe::allUserRoutines(compilation, model)) {
            auto m = computeRoutineMetrics(*sub);
            if (m.maxNesting <= limit) continue;
            diags.addViolation(
                "SV-MET-004", "DeepNesting",
                "Control structures in '" + std::string(sub->name) + "' nest " + std::to_string(m.maxNesting) +
                    " levels deep here (limit " + std::to_string(limit) + "). Return early or extract the "
                    "inner loop into its own task.",
                core::Severity::Warning, m.deepest.valid() ? m.deepest : sub->location);
        }
    }
};

} // namespace uvmcheck::rules::metrics

namespace uvmcheck::rules {
std::unique_ptr<Rule> createDeepNesting() {
    return std::make_unique<metrics::DeepNesting>();
}
}
