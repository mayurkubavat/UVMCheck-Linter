#include "uvmcheck/rules/Rule.h"
#include "uvmcheck/rules/RuleRegistry.h"
#include "uvmcheck/rules/SweSupport.h"
#include "slang/ast/expressions/AssignmentExpressions.h"
#include "slang/ast/statements/MiscStatements.h"

namespace uvmcheck::rules::design {

using namespace slang::ast;

namespace {

// The only statement of a body, looking through begin/end and statement lists.
const Statement* soleStatement(const Statement& stmt) {
    const Statement* s = &stmt;
    while (true) {
        if (s->kind == StatementKind::Block) {
            const auto& b = s->as<BlockStatement>();
            if (b.blockKind != StatementBlockKind::Sequential) return s;
            s = &b.body;
        } else if (s->kind == StatementKind::List) {
            const Statement* only = nullptr;
            for (const auto* item : s->as<StatementList>().list) {
                if (item->kind == StatementKind::Empty) continue;
                if (only) return nullptr;
                only = item;
            }
            if (!only) return nullptr;
            s = only;
        } else {
            return s;
        }
    }
}

const CallExpression* asCall(const Expression& expr) {
    const Expression& e = swe::stripConversions(expr);
    return e.kind == ExpressionKind::Call ? &e.as<CallExpression>() : nullptr;
}

// super.<name>(<same args>) as the whole body, possibly returned.
bool onlyCallsSuper(const SubroutineSymbol& m) {
    const Statement* s = soleStatement(m.getBody());
    if (!s) return false;
    const CallExpression* call = nullptr;
    if (s->kind == StatementKind::ExpressionStatement) call = asCall(s->as<ExpressionStatement>().expr);
    else if (s->kind == StatementKind::Return && s->as<ReturnStatement>().expr)
        call = asCall(*s->as<ReturnStatement>().expr);
    if (!call || call->isSystemCall() || call->getSubroutineName() != m.name) return false;
    if (!swe::exprText(*call).starts_with("super.")) return false;
    auto formals = m.getArguments();
    auto actuals = call->arguments();
    if (formals.size() != actuals.size()) return false;
    for (size_t i = 0; i < formals.size(); i++) {
        const Expression* a = actuals[i];
        if (a->kind == ExpressionKind::Assignment) a = &a->as<AssignmentExpression>().left();
        if (swe::referencedSymbol(*a) != formals[i]) return false;
    }
    return true;
}

} // namespace

class OverrideOnlyCallsSuper : public Rule {
public:
    std::string_view getId() const override { return "SV-DSN-006"; }
    std::string_view getName() const override { return "OverrideOnlyCallsSuper"; }
    std::string_view getDescription() const override {
        return "Flags overrides whose whole body is the super call with the same arguments, such as a "
               "connect_phase that only calls super.connect_phase(phase) (Pylint "
               "useless-parent-delegation, Sonar S1185). Constructors are kept.";
    }
    std::string_view getCategory() const override { return "Maintainability"; }
    core::Severity getDefaultSeverity() const override { return core::Severity::Info; }

    void run(Compilation&, const uvm::UvmModel& model, core::DiagnosticEngine& diags) override {
        for (const auto* cls : swe::userClasses(model)) {
            for (const auto* m : swe::classMethods(*cls)) {
                if (m->flags.has(MethodFlags::Constructor) || !onlyCallsSuper(*m)) continue;
                diags.addViolation(
                    "SV-DSN-006", "OverrideOnlyCallsSuper",
                    "'" + std::string(cls->name) + "::" + std::string(m->name) +
                        "' only calls super." + std::string(m->name) +
                        "() with the same arguments. The override adds nothing; delete it.",
                    core::Severity::Info, m->location);
            }
        }
    }
};

class EmptyOverrideOfConcrete : public Rule {
public:
    std::string_view getId() const override { return "SV-DSN-007"; }
    std::string_view getName() const override { return "EmptyOverrideOfConcrete"; }
    std::string_view getDescription() const override {
        return "Flags an empty override of a user base-class method that has a body, which silently "
               "switches the inherited behavior off (Fowler 'refused bequest', Liskov substitution).";
    }
    std::string_view getCategory() const override { return "Maintainability"; }
    core::Severity getDefaultSeverity() const override { return core::Severity::Warning; }

    void run(Compilation&, const uvm::UvmModel& model, core::DiagnosticEngine& diags) override {
        for (const auto* cls : swe::userClasses(model)) {
            for (const auto* m : swe::classMethods(*cls)) {
                const SubroutineSymbol* base = m->getOverride();
                if (!base || swe::isLibrarySymbol(*base) || base->flags.has(MethodFlags::Pure)) continue;
                if (swe::countStatements(m->getBody()) != 0) continue;
                if (swe::countStatements(base->getBody()) == 0) continue;
                const Symbol* baseCls = base->getParentScope() ? &base->getParentScope()->asSymbol() : nullptr;
                diags.addViolation(
                    "SV-DSN-007", "EmptyOverrideOfConcrete",
                    "'" + std::string(cls->name) + "::" + std::string(m->name) + "' is empty and replaces '" +
                        (baseCls ? std::string(baseCls->name) + "::" : std::string()) +
                        std::string(base->name) + "', which does work. Subclasses that switch inherited "
                        "behavior off break code that relies on the base contract; add a config knob "
                        "in the base class instead.",
                    core::Severity::Warning, m->location);
            }
        }
    }
};

} // namespace uvmcheck::rules::design

namespace uvmcheck::rules {
std::unique_ptr<Rule> createOverrideOnlyCallsSuper() {
    return std::make_unique<design::OverrideOnlyCallsSuper>();
}
std::unique_ptr<Rule> createEmptyOverrideOfConcrete() {
    return std::make_unique<design::EmptyOverrideOfConcrete>();
}
}
