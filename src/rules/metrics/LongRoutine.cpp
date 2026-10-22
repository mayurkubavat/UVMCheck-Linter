#include "uvmcheck/rules/Rule.h"
#include "uvmcheck/rules/RuleRegistry.h"
#include "uvmcheck/rules/CodeMetrics.h"
#include "uvmcheck/rules/SweSupport.h"

namespace uvmcheck::rules::metrics {

using namespace slang::ast;

class LongRoutine : public Rule {
public:
    std::string_view getId() const override { return "SV-MET-002"; }
    std::string_view getName() const override { return "LongRoutine"; }
    std::string_view getDescription() const override {
        return "Flags routines with more statements than the threshold (default 100; clang-tidy "
               "readability-function-size, Checkstyle MethodLength).";
    }
    std::string_view getCategory() const override { return "Maintainability"; }
    core::Severity getDefaultSeverity() const override { return core::Severity::Warning; }

    void run(Compilation& compilation, const uvm::UvmModel& model, core::DiagnosticEngine& diags) override {
        const int64_t limit = swe::ruleOption("SV-MET-002", "max_statements", 100);
        for (const auto* sub : swe::allUserRoutines(compilation, model)) {
            int n = swe::countStatements(sub->getBody());
            if (n <= limit) continue;
            diags.addViolation(
                "SV-MET-002", "LongRoutine",
                "'" + std::string(sub->name) + "' has " + std::to_string(n) + " statements (limit " +
                    std::to_string(limit) + "). Split it into smaller tasks or functions with one job each.",
                core::Severity::Warning, sub->location);
        }
    }
};

} // namespace uvmcheck::rules::metrics

namespace uvmcheck::rules {
std::unique_ptr<Rule> createLongRoutine() {
    return std::make_unique<metrics::LongRoutine>();
}
}
