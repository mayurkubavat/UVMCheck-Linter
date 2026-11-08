#include "uvmcheck/rules/Rule.h"
#include "uvmcheck/rules/RuleRegistry.h"
#include "slang/ast/ASTVisitor.h"
#include "slang/ast/TimingControl.h"
#include "slang/ast/statements/MiscStatements.h"
#include "slang/ast/expressions/MiscExpressions.h"
#include "slang/syntax/AllSyntax.h"
#include "slang/syntax/SyntaxVisitor.h"
#include <string>

namespace uvmcheck::rules::performance {

class ZeroDelayVisitor : public engine::AstVisitor<ZeroDelayVisitor> {
public:
    explicit ZeroDelayVisitor(core::DiagnosticEngine& diags) : diags(diags) {}

    void handle(const slang::ast::TimedStatement& stmt) {
        if (stmt.timing.kind == slang::ast::TimingControlKind::Delay) {
            const auto& delay = stmt.timing.as<slang::ast::DelayControl>();
            bool isZero = false;

            if (delay.expr.kind == slang::ast::ExpressionKind::IntegerLiteral) {
                if (delay.expr.as<slang::ast::IntegerLiteral>().getValue() == 0) {
                    isZero = true;
                }
            } else if (delay.expr.syntax && delay.expr.syntax->toString() == "0") {
                isZero = true;
            }

            if (isZero) {
                diags.addViolation(
                    "SV-PRF-004",
                    "ProceduralZeroDelay",
                    "Procedural delay '#0' reschedules the process into the Inactive simulation region. "
                    "Cascades of #0 delays cause delta-cycle thrashing, inflate event engine queues, and mask underlying race conditions. "
                    "Use non-blocking assignments (<=), clocking block skews, or explicit event synchronization instead.",
                    core::Severity::Warning,
                    stmt.sourceRange.start()
                );
            }
        }
        visitDefault(stmt);
    }

private:
    core::DiagnosticEngine& diags;
};

class ProceduralZeroDelay : public Rule {
public:
    std::string_view getId() const override { return "SV-PRF-004"; }
    std::string_view getName() const override { return "ProceduralZeroDelay"; }
    std::string_view getDescription() const override {
        return "Procedural delay #0 reschedules execution into the Inactive region. Repeated #0 rescheduling "
               "causes delta-cycle thrashing and degrades simulation throughput.";
    }
    std::string_view getCategory() const override { return "Performance"; }
    core::Severity getDefaultSeverity() const override { return core::Severity::Warning; }

    engine::RuleTask check(engine::RuleContext& ctx) override {
        auto& diags = ctx.diags();
        ZeroDelayVisitor visitor(diags);
        co_await ctx.visitAst(visitor);
    }
};

// A clock edge counts only when it gates the loop. An edge written inside the
// fork is the thread waiting, not the loop being clocked.
class EdgeOutsideFork : public slang::syntax::SyntaxVisitor<EdgeOutsideFork> {
public:
    bool found = false;

    void handle(const slang::syntax::SignalEventExpressionSyntax& ev) {
        const auto edge = ev.edge.valueText();
        if (edge == "posedge" || edge == "negedge") found = true;
        visitDefault(ev);
    }

    void handle(const slang::syntax::BlockStatementSyntax& block) {
        if (block.kind == slang::syntax::SyntaxKind::ParallelBlockStatement) return;
        visitDefault(block);
    }
};

bool isLoop(slang::syntax::SyntaxKind kind) {
    using slang::syntax::SyntaxKind;
    return kind == SyntaxKind::ForLoopStatement || kind == SyntaxKind::ForeachLoopStatement ||
           kind == SyntaxKind::LoopStatement || kind == SyntaxKind::DoWhileStatement ||
           kind == SyntaxKind::ForeverStatement;
}

bool loopIsClocked(const slang::syntax::SyntaxNode& loop) {
    EdgeOutsideFork edges;
    loop.visit(edges);
    return edges.found;
}

bool inClockedLoop(const slang::syntax::SyntaxNode& fork) {
    using slang::syntax::SyntaxKind;
    for (const auto* curr = fork.parent.get(); curr != nullptr; curr = curr->parent.get()) {
        if (curr->kind == SyntaxKind::AlwaysFFBlock) return true;
        if (curr->kind == SyntaxKind::AlwaysBlock || isLoop(curr->kind)) {
            if (loopIsClocked(*curr)) return true;
        }
    }
    return false;
}

class ForkInClockedLoopVisitor : public engine::SyntaxVisitor<ForkInClockedLoopVisitor> {
public:
    explicit ForkInClockedLoopVisitor(core::DiagnosticEngine& diags) : diags(diags) {}

    void handle(const slang::syntax::BlockStatementSyntax& block) {
        if (block.kind == slang::syntax::SyntaxKind::ParallelBlockStatement && inClockedLoop(block)) {
            const auto joinKw = std::string(block.end.valueText());
            diags.addViolation(
                "SV-PRF-013", "ForkInClockedLoop",
                "fork ... " + joinKw +
                    " sits inside a clocked loop, so every clock starts another thread. "
                    "Do the work in the clocked process, or start the fork once outside the loop.",
                core::Severity::Warning, block.begin.location());
        }
        visitDefault(block);
    }

private:
    core::DiagnosticEngine& diags;
};

class ForkInClockedLoop : public Rule {
public:
    std::string_view getId() const override { return "SV-PRF-013"; }
    std::string_view getName() const override { return "ForkInClockedLoop"; }
    std::string_view getDescription() const override {
        return "Flags fork/join, join_any, and join_none inside a clocked loop "
               "(always_ff, always @(posedge/negedge), or a loop that waits on a clock edge). "
               "A thread started on every clock piles work onto the scheduler.";
    }
    std::string_view getCategory() const override { return "Performance"; }
    core::Severity getDefaultSeverity() const override { return core::Severity::Warning; }

    engine::RuleTask check(engine::RuleContext& ctx) override {
        auto& diags = ctx.diags();
        ForkInClockedLoopVisitor visitor(diags);
        co_await ctx.visitSyntax(visitor);
    }
};

} // namespace uvmcheck::rules::performance

namespace uvmcheck::rules {
std::unique_ptr<Rule> createProceduralZeroDelay() {
    return std::make_unique<performance::ProceduralZeroDelay>();
}
std::unique_ptr<Rule> createForkInClockedLoop() {
    return std::make_unique<performance::ForkInClockedLoop>();
}
}

UVMCHECK_REGISTER_RULE(uvmcheck::rules::createProceduralZeroDelay)
UVMCHECK_REGISTER_RULE(uvmcheck::rules::createForkInClockedLoop)
