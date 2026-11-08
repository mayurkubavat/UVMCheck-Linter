#include "uvmcheck/rules/Rule.h"
#include "uvmcheck/rules/RuleRegistry.h"
#include "uvmcheck/rules/SweSupport.h"
#include "slang/ast/expressions/CallExpression.h"
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

class UnusedLocalMethod : public Rule {
public:
    std::string_view getId() const override { return "SV-DED-014"; }
    std::string_view getName() const override { return "UnusedLocalMethod"; }
    std::string_view getDescription() const override {
        return "Flags a local task or function that no method of its class calls. A local method is "
               "invisible outside the class, so with no caller it is dead.";
    }
    std::string_view getCategory() const override { return "Maintainability"; }
    core::Severity getDefaultSeverity() const override { return core::Severity::Warning; }

    void run(Compilation&, const uvm::UvmModel& model, core::DiagnosticEngine& diags) override {
        for (const auto* cls : swe::userClasses(model)) {
            auto methods = swe::classMethods(*cls);
            for (const auto* target : methods) {
                if (target->visibility != Visibility::Local) continue;
                if (target->flags.has(MethodFlags::Virtual) || target->flags.has(MethodFlags::Constructor) ||
                    target->flags.has(MethodFlags::BuiltIn))
                    continue;
                if (swe::countStatements(target->getBody()) == 0) continue;
                bool used = false;
                for (const auto* caller : methods) {
                    if (caller == target) continue;
                    swe::forEachCall(caller->getBody(), [&](const CallExpression& call) {
                        if (call.isSystemCall()) return;
                        if (const auto* sub = std::get_if<const SubroutineSymbol*>(&call.subroutine)) {
                            if (*sub == target) used = true;
                        }
                    });
                    if (used) break;
                }
                if (used) continue;
                diags.addViolation(
                    "SV-DED-014", "UnusedLocalMethod",
                    "Local method '" + std::string(cls->name) + "::" + std::string(target->name) +
                        "' has no caller in the class. Remove it, or call it from the method that "
                        "should use it.",
                    core::Severity::Warning, target->location);
            }
        }
    }
};

} // namespace uvmcheck::rules::deadcode

namespace uvmcheck::rules {
std::unique_ptr<Rule> createUnusedRoutineArgument() {
    return std::make_unique<deadcode::UnusedRoutineArgument>();
}
std::unique_ptr<Rule> createUnusedLocalMethod() {
    return std::make_unique<deadcode::UnusedLocalMethod>();
}
}

UVMCHECK_REGISTER_RULE(uvmcheck::rules::createUnusedRoutineArgument)
UVMCHECK_REGISTER_RULE(uvmcheck::rules::createUnusedLocalMethod)
