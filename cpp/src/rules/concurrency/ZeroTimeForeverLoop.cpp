// SV-DLY-004: a forever loop whose body can never let time advance.

#include "uvmcheck/rules/Rule.h"
#include "uvmcheck/rules/RuleRegistry.h"

#include "slang/ast/ASTVisitor.h"
#include "slang/ast/expressions/CallExpression.h"
#include "slang/ast/statements/LoopStatements.h"
#include "slang/ast/statements/MiscStatements.h"
#include "slang/ast/symbols/SubroutineSymbols.h"

namespace uvmcheck::rules::concurrency {

using namespace slang::ast;

namespace {

/// Looks for anything in a loop body that can block or leave the loop.
struct BlockingScan : public ASTVisitor<BlockingScan, VisitFlags::AllGood> {
    bool found = false;

    void handle(const TimedStatement&) { found = true; }
    void handle(const WaitStatement&) { found = true; }
    void handle(const WaitForkStatement&) { found = true; }
    void handle(const WaitOrderStatement&) { found = true; }
    void handle(const DisableStatement&) { found = true; }
    void handle(const BreakStatement&) { found = true; }
    void handle(const ReturnStatement&) { found = true; }
    void handle(const CallExpression& call) {
        if (call.getSubroutineKind() == SubroutineKind::Task) {
            // A user task may block; $display and friends never do, but $finish/$stop end it.
            if (!call.isSystemCall()) found = true;
            else if (auto n = call.getSubroutineName(); n == "$finish" || n == "$stop" || n == "$fatal")
                found = true;
        }
        visitDefault(call);
    }
};

class ForeverVisitor : public engine::AstVisitor<ForeverVisitor> {
public:
    explicit ForeverVisitor(core::DiagnosticEngine& diags) : diags(diags) {}

    void handle(const ForeverLoopStatement& loop) {
        BlockingScan scan;
        loop.body.visit(scan);
        if (!scan.found) {
            diags.addViolation(
                "SV-DLY-004", "ZeroTimeForeverLoop",
                "forever loop with no timing control (#, @, wait), no task call and no way out (break, "
                "return, disable). It runs forever in zero simulation time and hangs the simulator. "
                "Wait on a clock or event inside the loop, or call the blocking task it is meant to wait in.",
                core::Severity::Error, loop.sourceRange.start());
        }
        visitDefault(loop);
    }

private:
    core::DiagnosticEngine& diags;
};

} // namespace

class ZeroTimeForeverLoop : public Rule {
public:
    std::string_view getId() const override { return "SV-DLY-004"; }
    std::string_view getName() const override { return "ZeroTimeForeverLoop"; }
    std::string_view getDescription() const override {
        return "Flags forever loops whose body has no timing control, no task call and no exit, so the "
               "loop spins in zero simulation time and hangs the simulator.";
    }
    std::string_view getCategory() const override { return "Concurrency"; }
    core::Severity getDefaultSeverity() const override { return core::Severity::Error; }

    engine::RuleTask check(engine::RuleContext& ctx) override {
        ForeverVisitor visitor(ctx.diags());
        co_await ctx.visitAst(visitor);
    }
};

} // namespace uvmcheck::rules::concurrency

namespace uvmcheck::rules {
std::unique_ptr<Rule> createZeroTimeForeverLoop() {
    return std::make_unique<concurrency::ZeroTimeForeverLoop>();
}
} // namespace uvmcheck::rules

UVMCHECK_REGISTER_RULE(uvmcheck::rules::createZeroTimeForeverLoop)
