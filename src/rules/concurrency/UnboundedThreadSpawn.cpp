#include "uvmcheck/rules/Rule.h"
#include "uvmcheck/rules/RuleRegistry.h"
#include "uvmcheck/rules/SweSupport.h"
#include "slang/ast/statements/LoopStatements.h"
#include "slang/ast/statements/MiscStatements.h"
#include "slang/syntax/SyntaxNode.h"

namespace uvmcheck::rules::concurrency {

using namespace slang::ast;

namespace {

// A one-branch fork's range starts at its branch; point at the fork keyword instead.
slang::SourceLocation forkLoc(const BlockStatement& b) {
    return b.syntax ? b.syntax->sourceRange().start() : swe::locOf(b);
}

// Looks for join_none forks in a loop body and for anything that bounds them.
struct SpawnFacts : public ASTVisitor<SpawnFacts, VisitFlags::AllGood> {
    const BlockStatement* spawn = nullptr;
    bool bounded = false;

    void handle(const BlockStatement& b) {
        if (b.blockKind == StatementBlockKind::JoinNone) {
            if (!spawn) spawn = &b;
            return; // what the thread does inside is its own business
        }
        visitDefault(b);
    }
    void handle(const WaitForkStatement&) { bounded = true; }
    void handle(const DisableForkStatement&) { bounded = true; }
    void handle(const CallExpression& call) {
        if (swe::isSemaphoreGet(call)) bounded = true;
        visitDefault(call);
    }
};

class SpawnVisitor : public ASTVisitor<SpawnVisitor, VisitFlags::AllGood> {
public:
    SpawnVisitor(const std::string& where, core::DiagnosticEngine& diags) : where(where), diags(diags) {}

    void handle(const ForeverLoopStatement& loop) {
        check("forever", loop.body);
        visitDefault(loop);
    }
    void handle(const WhileLoopStatement& loop) {
        check("while", loop.body);
        visitDefault(loop);
    }
    void handle(const DoWhileLoopStatement& loop) {
        check("do-while", loop.body);
        visitDefault(loop);
    }

private:
    const std::string& where;
    core::DiagnosticEngine& diags;

    void check(const char* kind, const Statement& body) {
        SpawnFacts f;
        body.visit(f);
        if (!f.spawn || f.bounded) return;
        diags.addViolation(
            "SV-THR-003", "UnboundedThreadSpawn",
            std::string("fork ... join_none inside a ") + kind + " loop in " + where +
                " starts a new thread every iteration with nothing that limits how many run at once. "
                "Threads pile up when the consumer is slow and are still running at end of test; bound "
                "them with a semaphore, or wait fork before the next batch.",
            core::Severity::Warning, forkLoc(*f.spawn));
    }
};

} // namespace

class UnboundedThreadSpawn : public Rule {
public:
    std::string_view getId() const override { return "SV-THR-003"; }
    std::string_view getName() const override { return "UnboundedThreadSpawn"; }
    std::string_view getDescription() const override {
        return "Flags fork ... join_none inside forever or while loops with no wait fork, disable fork "
               "or semaphore get in the loop to bound outstanding threads (thread-leak checks).";
    }
    std::string_view getCategory() const override { return "Concurrency"; }
    core::Severity getDefaultSeverity() const override { return core::Severity::Warning; }

    void run(Compilation& compilation, const uvm::UvmModel& model, core::DiagnosticEngine& diags) override {
        for (const auto* r : swe::allUserRoutines(compilation, model)) {
            std::string where = "'" + std::string(r->name) + "'";
            SpawnVisitor v(where, diags);
            r->getBody().visit(v);
        }
    }
};

} // namespace uvmcheck::rules::concurrency

namespace uvmcheck::rules {
std::unique_ptr<Rule> createUnboundedThreadSpawn() {
    return std::make_unique<concurrency::UnboundedThreadSpawn>();
}
}
