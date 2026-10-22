#include "uvmcheck/rules/Rule.h"
#include "uvmcheck/rules/RuleRegistry.h"
#include "uvmcheck/rules/CodeMetrics.h"
#include "uvmcheck/rules/SweSupport.h"

namespace uvmcheck::rules::metrics {

using namespace slang::ast;

class TooManyArguments : public Rule {
public:
    std::string_view getId() const override { return "SV-MET-003"; }
    std::string_view getName() const override { return "TooManyArguments"; }
    std::string_view getDescription() const override {
        return "Flags tasks and functions with more formal arguments than the threshold (default 7; Pylint "
               "R0913, Sonar S107). Overrides are judged at the base declaration only.";
    }
    std::string_view getCategory() const override { return "Maintainability"; }
    core::Severity getDefaultSeverity() const override { return core::Severity::Warning; }

    void run(Compilation& compilation, const uvm::UvmModel& model, core::DiagnosticEngine& diags) override {
        const int64_t limit = swe::ruleOption("SV-MET-003", "max_args", 7);
        for (const auto* sub : swe::allUserRoutines(compilation, model)) {
            if (sub->getOverride() || sub->flags.has(MethodFlags::Constructor)) continue;
            int n = static_cast<int>(sub->getArguments().size());
            if (n <= limit) continue;
            diags.addViolation(
                "SV-MET-003", "TooManyArguments",
                "'" + std::string(sub->name) + "' takes " + std::to_string(n) + " arguments (limit " +
                    std::to_string(limit) + "). Pass the transaction or a config object instead, so adding "
                    "a protocol field does not change every caller.",
                core::Severity::Warning, sub->location);
        }
    }
};

} // namespace uvmcheck::rules::metrics

namespace uvmcheck::rules {
std::unique_ptr<Rule> createTooManyArguments() {
    return std::make_unique<metrics::TooManyArguments>();
}
}
