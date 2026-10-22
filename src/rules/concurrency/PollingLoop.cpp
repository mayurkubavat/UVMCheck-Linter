#include "uvmcheck/rules/Rule.h"
#include "uvmcheck/rules/RuleRegistry.h"
#include "uvmcheck/rules/SweSupport.h"
#include "slang/ast/statements/ConditionalStatements.h"
#include "slang/ast/statements/LoopStatements.h"
#include "slang/ast/statements/MiscStatements.h"

namespace uvmcheck::rules::concurrency {

using namespace slang::ast;

namespace {

// What a loop body does that matters for polling: fixed waits, real waits, task calls, exits.
struct LoopFacts : public ASTVisitor<LoopFacts, VisitFlags::AllGood> {
    bool fixedDelay = false;
    bool realWait = false;   // @event, wait(), task call: something that can block on a change
    std::vector<const Expression*> exitConditions;

    void handle(const TimedStatement& t) {
        if (t.timing.kind == TimingControlKind::Delay) {
            if (swe::isFixedDelay(t.timing)) fixedDelay = true;
            else realWait = true;
        } else {
            realWait = true;
        }
        visitDefault(t);
    }
    void handle(const WaitStatement&) { realWait = true; }
    void handle(const WaitOrderStatement&) { realWait = true; }
    void handle(const CallExpression& call) {
        if (!call.isSystemCall()) {
            const auto* sub = std::get_if<const SubroutineSymbol*>(&call.subroutine);
            if (sub && *sub && (*sub)->subroutineKind == SubroutineKind::Task) realWait = true;
        }
        visitDefault(call);
    }
    void handle(const ConditionalStatement& c) {
        if (exits(c.ifTrue)) {
            for (const auto& cond : c.conditions) exitConditions.push_back(cond.expr);
        }
        visitDefault(c);
    }

    static bool exits(const Statement& s) {
        struct Finder : public ASTVisitor<Finder, VisitFlags::AllGood> {
            bool found = false;
            void handle(const BreakStatement&) { found = true; }
            void handle(const ReturnStatement&) { found = true; }
        } f;
        s.visit(f);
        return f.found;
    }
};

class PollingVisitor : public ASTVisitor<PollingVisitor, VisitFlags::AllGood> {
public:
    PollingVisitor(const std::string& where, core::DiagnosticEngine& diags) : where(where), diags(diags) {}

    void handle(const WhileLoopStatement& loop) {
        check(loop, loop.body, &loop.cond);
        visitDefault(loop);
    }
    void handle(const DoWhileLoopStatement& loop) {
        check(loop, loop.body, &loop.cond);
        visitDefault(loop);
    }
    void handle(const ForeverLoopStatement& loop) {
        check(loop, loop.body, nullptr);
        visitDefault(loop);
    }

private:
    const std::string& where;
    core::DiagnosticEngine& diags;

    void check(const Statement& loop, const Statement& body, const Expression* cond) {
        LoopFacts facts;
        body.visit(facts);
        if (!facts.fixedDelay || facts.realWait) return;
        std::vector<const Expression*> conds;
        if (cond) {
            if (swe::isConstantExpr(*cond)) return;
            conds.push_back(cond);
        } else {
            conds = facts.exitConditions; // forever: polling only if it can leave
        }
        if (conds.empty()) return;
        // A loop that changes what it tests (a counter, a timeout) is timed work, not polling.
        auto written = swe::writtenSymbols(body);
        for (const auto* c : conds) {
            bool changedHere = false;
            for (const auto* s : swe::referencedSymbols(*c)) changedHere |= written.count(s) > 0;
            if (changedHere) continue;
            diags.addViolation(
                "SV-CNC-003", "PollingLoop",
                "Loop in " + where + " polls '" + swe::exprText(*c) + "' with a fixed delay. Polling "
                    "ties the check to the timescale, misses short pulses and slows simulation; wait for "
                    "the change instead with wait(" + swe::exprText(*c) + "), @(...) on the signal or "
                    "clocking block, or an event.",
                core::Severity::Warning, swe::locOf(loop));
            return;
        }
    }
};

} // namespace

class PollingLoop : public Rule {
public:
    std::string_view getId() const override { return "SV-CNC-003"; }
    std::string_view getName() const override { return "PollingLoop"; }
    std::string_view getDescription() const override {
        return "Flags loops that test a flag or signal and wait a constant #delay between tests, "
               "instead of wait(), @ or an event (busy-wait smell, Sonar S2925).";
    }
    std::string_view getCategory() const override { return "Concurrency"; }
    core::Severity getDefaultSeverity() const override { return core::Severity::Warning; }

    void run(Compilation& compilation, const uvm::UvmModel& model, core::DiagnosticEngine& diags) override {
        for (const auto* r : swe::allUserRoutines(compilation, model)) {
            std::string where = "'" + std::string(r->name) + "'";
            PollingVisitor v(where, diags);
            r->getBody().visit(v);
        }
    }
};

} // namespace uvmcheck::rules::concurrency

namespace uvmcheck::rules {
std::unique_ptr<Rule> createPollingLoop() {
    return std::make_unique<concurrency::PollingLoop>();
}
}
