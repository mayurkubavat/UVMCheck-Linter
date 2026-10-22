#include "uvmcheck/rules/Rule.h"
#include "uvmcheck/rules/RuleRegistry.h"
#include "uvmcheck/rules/SweSupport.h"

namespace uvmcheck::rules::concurrency {

using namespace slang::ast;

class FileHandleNotClosed : public Rule {
public:
    std::string_view getId() const override { return "SV-RSC-002"; }
    std::string_view getName() const override { return "FileHandleNotClosed"; }
    std::string_view getDescription() const override {
        return "Flags $fopen calls in classes that never call $fclose, which loses buffered trace data "
               "and exhausts file handles in long regressions (Sonar S2095).";
    }
    std::string_view getCategory() const override { return "Correctness"; }
    core::Severity getDefaultSeverity() const override { return core::Severity::Warning; }

    void run(Compilation&, const uvm::UvmModel& model, core::DiagnosticEngine& diags) override {
        for (const auto* cls : swe::userClasses(model)) {
            std::vector<const CallExpression*> opens;
            bool closes = false;
            for (const auto* m : swe::classMethods(*cls)) {
                swe::forEachCall(m->getBody(), [&](const CallExpression& call) {
                    if (!call.isSystemCall()) return;
                    if (call.getSubroutineName() == "$fopen") opens.push_back(&call);
                    else if (call.getSubroutineName() == "$fclose") closes = true;
                });
            }
            if (closes) continue;
            for (const auto* call : opens) {
                diags.addViolation(
                    "SV-RSC-002", "FileHandleNotClosed",
                    "'" + std::string(cls->name) + "' opens a file with " + swe::exprText(*call) +
                        " but never calls $fclose. Close it in final_phase or report_phase so buffered "
                        "output is flushed and the handle is released.",
                    core::Severity::Warning, swe::locOf(*call));
            }
        }
    }
};

} // namespace uvmcheck::rules::concurrency

namespace uvmcheck::rules {
std::unique_ptr<Rule> createFileHandleNotClosed() {
    return std::make_unique<concurrency::FileHandleNotClosed>();
}
}
