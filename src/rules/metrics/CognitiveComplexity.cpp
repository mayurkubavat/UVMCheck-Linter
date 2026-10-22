#include "uvmcheck/rules/Rule.h"
#include "uvmcheck/rules/RuleRegistry.h"
#include "uvmcheck/rules/CodeMetrics.h"
#include "uvmcheck/rules/SweSupport.h"

namespace uvmcheck::rules::metrics {

using namespace slang::ast;

class CognitiveComplexity : public Rule {
public:
    std::string_view getId() const override { return "SV-MET-001"; }
    std::string_view getName() const override { return "CognitiveComplexity"; }
    std::string_view getDescription() const override {
        return "Flags routines whose Sonar cognitive complexity (nesting-weighted branching, with fork/join "
               "counted as nesting) exceeds the threshold (default 25; Sonar S3776).";
    }
    std::string_view getCategory() const override { return "Maintainability"; }
    core::Severity getDefaultSeverity() const override { return core::Severity::Warning; }

    void run(Compilation& compilation, const uvm::UvmModel& model, core::DiagnosticEngine& diags) override {
        const int64_t limit = swe::ruleOption("SV-MET-001", "threshold", 25);
        for (const auto* sub : swe::allUserRoutines(compilation, model)) {
            auto m = computeRoutineMetrics(*sub);
            if (m.cognitive <= limit) continue;
            diags.addViolation(
                "SV-MET-001", "CognitiveComplexity",
                "'" + std::string(sub->name) + "' has cognitive complexity " + std::to_string(m.cognitive) +
                    " (limit " + std::to_string(limit) + "). Deeply nested forks, loops and branches are "
                    "hard to review and to cover; extract the nested parts into named tasks.",
                core::Severity::Warning, sub->location);
        }
    }
};

} // namespace uvmcheck::rules::metrics

namespace uvmcheck::rules {
std::unique_ptr<Rule> createCognitiveComplexity() {
    return std::make_unique<metrics::CognitiveComplexity>();
}
}
