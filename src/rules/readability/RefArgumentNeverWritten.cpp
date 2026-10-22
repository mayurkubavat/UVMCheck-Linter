#include "uvmcheck/rules/Rule.h"
#include "uvmcheck/rules/RuleRegistry.h"
#include "uvmcheck/rules/SweSupport.h"

namespace uvmcheck::rules::readability {

using namespace slang::ast;


class RefArgumentNeverWritten : public Rule {
public:
    std::string_view getId() const override { return "SV-API-001"; }
    std::string_view getName() const override { return "RefArgumentNeverWritten"; }
    std::string_view getDescription() const override {
        return "Flags ref arguments that the routine only reads; declare them const ref so callers "
               "know they are not modified (clang-tidy readability-non-const-parameter). Virtual "
               "methods are skipped because overrides share the signature.";
    }
    std::string_view getCategory() const override { return "Maintainability"; }
    core::Severity getDefaultSeverity() const override { return core::Severity::Info; }

    void run(Compilation& compilation, const uvm::UvmModel& model, core::DiagnosticEngine& diags) override {
        for (const auto* r : swe::allUserRoutines(compilation, model)) {
            if (r->isVirtual() || r->flags.has(MethodFlags::DPIImport)) continue;
            std::vector<const FormalArgumentSymbol*> refs;
            for (const auto* a : r->getArguments()) {
                if (a->direction == ArgumentDirection::Ref && !a->flags.has(VariableFlags::Const)) refs.push_back(a);
            }
            if (refs.empty()) continue;
            auto written = swe::writtenSymbols(r->getBody());
            for (const auto* a : refs) {
                if (written.count(a)) continue;
                diags.addViolation(
                    "SV-API-001", "RefArgumentNeverWritten",
                    "'" + std::string(a->name) + "' is passed by ref to '" + std::string(r->name) +
                        "' but never modified. Declare it 'const ref' so callers can see the routine "
                        "only reads it.",
                    core::Severity::Info, a->location);
            }
        }
    }
};

} // namespace uvmcheck::rules::readability

namespace uvmcheck::rules {
std::unique_ptr<Rule> createRefArgumentNeverWritten() {
    return std::make_unique<readability::RefArgumentNeverWritten>();
}
}
