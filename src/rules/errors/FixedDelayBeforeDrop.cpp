#include "uvmcheck/rules/Rule.h"
#include "uvmcheck/rules/RuleRegistry.h"
#include "uvmcheck/rules/SweSupport.h"
#include "slang/ast/statements/MiscStatements.h"

namespace uvmcheck::rules::errors {

using namespace slang::ast;

namespace {

bool isDropCall(const Statement& stmt) {
    if (stmt.kind != StatementKind::ExpressionStatement) return false;
    const Expression& e = swe::stripConversions(stmt.as<ExpressionStatement>().expr);
    return e.kind == ExpressionKind::Call && e.as<CallExpression>().getSubroutineName() == "drop_objection";
}

class DelayVisitor : public ASTVisitor<DelayVisitor, VisitFlags::AllGood> {
public:
    explicit DelayVisitor(core::DiagnosticEngine& diags) : diags(diags) {}

    void handle(const StatementList& list) {
        for (size_t i = 0; i + 1 < list.list.size(); i++) {
            const Statement* s = list.list[i];
            if (s && s->kind == StatementKind::Timed && isDropCall(*list.list[i + 1])) {
                const auto& t = s->as<TimedStatement>();
                if (t.stmt.kind == StatementKind::Empty && swe::isFixedDelay(t.timing)) report(t);
            }
        }
        visitDefault(list);
    }

    void handle(const TimedStatement& t) {
        if (isDropCall(t.stmt) && swe::isFixedDelay(t.timing)) report(t);
        visitDefault(t);
    }

private:
    core::DiagnosticEngine& diags;

    void report(const TimedStatement& t) {
        diags.addViolation(
            "UVM-TST-002", "FixedDelayBeforeDrop",
            "Fixed delay '" + swe::exprText(t.timing.as<DelayControl>().expr) +
                "' before drop_objection() guesses how long the testbench needs to drain. It is flaky "
                "when traffic grows and wastes time when it shrinks. Use the objection drain time "
                "(phase.phase_done.set_drain_time) or phase_ready_to_end() in the scoreboard.",
            core::Severity::Warning, swe::locOf(t));
    }
};

} // namespace

class FixedDelayBeforeDrop : public Rule {
public:
    std::string_view getId() const override { return "UVM-TST-002"; }
    std::string_view getName() const override { return "FixedDelayBeforeDrop"; }
    std::string_view getDescription() const override {
        return "Flags a constant #delay immediately before drop_objection(), used to let traffic drain "
               "(xUnit 'sleepy test', Sonar S2925).";
    }
    std::string_view getCategory() const override { return "UVM"; }
    core::Severity getDefaultSeverity() const override { return core::Severity::Warning; }

    void run(Compilation&, const uvm::UvmModel& model, core::DiagnosticEngine& diags) override {
        for (const auto* cls : swe::userClasses(model)) {
            for (const auto* m : swe::classMethods(*cls)) {
                DelayVisitor v(diags);
                m->getBody().visit(v);
            }
        }
    }
};

} // namespace uvmcheck::rules::errors

namespace uvmcheck::rules {
std::unique_ptr<Rule> createFixedDelayBeforeDrop() {
    return std::make_unique<errors::FixedDelayBeforeDrop>();
}
}
