#include "uvmcheck/rules/Rule.h"
#include "uvmcheck/rules/RuleRegistry.h"
#include "slang/ast/ASTVisitor.h"
#include "slang/ast/statements/LoopStatements.h"
#include "slang/ast/statements/MiscStatements.h"
#include "slang/syntax/AllSyntax.h"

namespace uvmcheck::rules::concurrency {

class LoopTimeAdvanceVisitor : public slang::ast::ASTVisitor<LoopTimeAdvanceVisitor, slang::ast::VisitFlags::AllGood> {
public:
    void handle(const slang::ast::TimedStatement& stmt) {
        hasTimeAdvance = true;
        visitDefault(stmt);
    }

    void handle(const slang::ast::WaitStatement& stmt) {
        if (stmt.cond.syntax) {
            std::string condStr(stmt.cond.syntax->toString());
            if (condStr.find(".triggered") != std::string::npos) {
                hasEventTriggerWait = true;
                waitLoc = stmt.sourceRange.start();
            }
        }
        visitDefault(stmt);
    }

    bool hasTimeAdvance = false;
    bool hasEventTriggerWait = false;
    slang::SourceLocation waitLoc;
};

class ZeroDelayEventVisitor : public slang::ast::ASTVisitor<ZeroDelayEventVisitor, slang::ast::VisitFlags::AllGood> {
public:
    explicit ZeroDelayEventVisitor(core::DiagnosticEngine& diags) : diags(diags) {}

    void handle(const slang::ast::ForeverLoopStatement& loop) {
        checkLoopBody(loop.body, loop.sourceRange.start());
        visitDefault(loop);
    }

    void handle(const slang::ast::WhileLoopStatement& loop) {
        checkLoopBody(loop.body, loop.sourceRange.start());
        visitDefault(loop);
    }

    void handle(const slang::ast::DoWhileLoopStatement& loop) {
        checkLoopBody(loop.body, loop.sourceRange.start());
        visitDefault(loop);
    }

private:
    void checkLoopBody(const slang::ast::Statement& body, slang::SourceLocation loopLoc) {
        LoopTimeAdvanceVisitor visitor;
        body.visit(visitor);

        if (visitor.hasEventTriggerWait && !visitor.hasTimeAdvance) {
            diags.addViolation(
                "SV-EVT-001",
                "ZeroDelayEventWaitLoop",
                "Loop contains 'wait(event.triggered)' without an intervening simulation time advance (@ or #). "
                "In SystemVerilog (IEEE 1800 Sec 15.5.2), 'event.triggered' remains high for the entire current time-step across all delta cycles. "
                "Without a time-advancing delay, this loop executes infinitely in the same delta cycle, deadlocking the simulator.",
                core::Severity::Error,
                visitor.waitLoc.valid() ? visitor.waitLoc : loopLoc
            );
        }
    }

    core::DiagnosticEngine& diags;
};

class ZeroDelayEventWaitLoop : public Rule {
public:
    std::string_view getId() const override { return "SV-EVT-001"; }
    std::string_view getName() const override { return "ZeroDelayEventWaitLoop"; }
    std::string_view getDescription() const override {
        return "Using wait(event.triggered) inside a loop without an intervening time-advancing delay (@ or #) "
               "causes a zero-delay infinite delta loop that hangs the simulator at 100% CPU.";
    }
    std::string_view getCategory() const override { return "Concurrency"; }
    core::Severity getDefaultSeverity() const override { return core::Severity::Error; }

    void run(slang::ast::Compilation& compilation,
             const uvmcheck::uvm::UvmModel& /*uvmModel*/,
             core::DiagnosticEngine& diags) override {
        ZeroDelayEventVisitor visitor(diags);
        compilation.getRoot().visit(visitor);
    }
};

} // namespace uvmcheck::rules::concurrency

namespace uvmcheck::rules {
std::unique_ptr<Rule> createZeroDelayEventWaitLoop() {
    return std::make_unique<concurrency::ZeroDelayEventWaitLoop>();
}
}
