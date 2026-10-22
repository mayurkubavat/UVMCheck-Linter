#include "uvmcheck/rules/Rule.h"
#include "uvmcheck/rules/RuleRegistry.h"
#include "uvmcheck/rules/SweSupport.h"
#include "slang/ast/expressions/MiscExpressions.h"

#include <unordered_set>

namespace uvmcheck::rules::deadcode {

using namespace slang::ast;

namespace {

class ReferenceCollector : public ASTVisitor<ReferenceCollector, VisitFlags::AllGood> {
public:
    std::unordered_set<const Symbol*> used;
    void handle(const NamedValueExpression& e) {
        used.insert(&e.symbol);
        visitDefault(e);
    }
};

} // namespace

class UnusedRoutineArgument : public Rule {
public:
    std::string_view getId() const override { return "SV-DED-013"; }
    std::string_view getName() const override { return "UnusedRoutineArgument"; }
    std::string_view getDescription() const override {
        return "Flags task and function arguments that the body never uses (clang-tidy "
               "misc-unused-parameters, Pylint W0613). Virtual methods and overrides, phase methods, "
               "DPI routines and empty bodies are skipped.";
    }
    std::string_view getCategory() const override { return "Maintainability"; }
    core::Severity getDefaultSeverity() const override { return core::Severity::Info; }

    void run(Compilation& compilation, const uvm::UvmModel& model, core::DiagnosticEngine& diags) override {
        for (const auto* r : swe::allUserRoutines(compilation, model)) {
            if (r->isVirtual() || r->flags.has(MethodFlags::DPIImport) || r->name.ends_with("_phase")) continue;
            auto args = r->getArguments();
            if (args.empty() || swe::countStatements(r->getBody()) == 0) continue;
            ReferenceCollector refs;
            r->getBody().visit(refs);
            for (const auto* a : args) {
                if (a->name.empty() || a->name.starts_with("_") || a->name.starts_with("unused")) continue;
                if (refs.used.count(a)) continue;
                diags.addViolation(
                    "SV-DED-013", "UnusedRoutineArgument",
                    "Argument '" + std::string(a->name) + "' of '" + std::string(r->name) +
                        "' is never used. Remove it, or use it if the caller expects it to have an "
                        "effect.",
                    core::Severity::Info, a->location);
            }
        }
    }
};

} // namespace uvmcheck::rules::deadcode

namespace uvmcheck::rules {
std::unique_ptr<Rule> createUnusedRoutineArgument() {
    return std::make_unique<deadcode::UnusedRoutineArgument>();
}
}
