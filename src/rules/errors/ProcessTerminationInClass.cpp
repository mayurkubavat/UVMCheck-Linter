#include "uvmcheck/rules/Rule.h"
#include "uvmcheck/rules/RuleRegistry.h"
#include "uvmcheck/rules/SweSupport.h"

namespace uvmcheck::rules::errors {

using namespace slang::ast;

class ProcessTerminationInClass : public Rule {
public:
    std::string_view getId() const override { return "SV-ERR-003"; }
    std::string_view getName() const override { return "ProcessTerminationInClass"; }
    std::string_view getDescription() const override {
        return "Flags $finish, $stop and $exit called from class methods. The run then skips the "
               "extract, check and report phases and the UVM report summary (Sonar S1147).";
    }
    std::string_view getCategory() const override { return "Correctness"; }
    core::Severity getDefaultSeverity() const override { return core::Severity::Warning; }

    void run(Compilation&, const uvm::UvmModel& model, core::DiagnosticEngine& diags) override {
        for (const auto* cls : swe::userClasses(model)) {
            for (const auto* m : swe::classMethods(*cls)) {
                swe::forEachCall(m->getBody(), [&](const CallExpression& call) {
                    if (!call.isSystemCall()) return;
                    std::string_view n = call.getSubroutineName();
                    if (n != "$finish" && n != "$stop" && n != "$exit") return;
                    diags.addViolation(
                        "SV-ERR-003", "ProcessTerminationInClass",
                        std::string(n) + " in " + std::string(cls->name) + "::" + std::string(m->name) +
                            " ends the simulation without the check and report phases or the UVM "
                            "summary, so regression scripts may read the log as a pass. Use `uvm_fatal "
                            "to abort, or drop objections to end the test.",
                        core::Severity::Warning, swe::locOf(call));
                });
            }
        }
    }
};

} // namespace uvmcheck::rules::errors

namespace uvmcheck::rules {
std::unique_ptr<Rule> createProcessTerminationInClass() {
    return std::make_unique<errors::ProcessTerminationInClass>();
}
}
