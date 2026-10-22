#include "uvmcheck/rules/Rule.h"
#include "uvmcheck/rules/RuleRegistry.h"
#include "slang/syntax/AllSyntax.h"
#include "slang/syntax/SyntaxTree.h"
#include "slang/syntax/SyntaxVisitor.h"
#include <string>

namespace uvmcheck::rules::performance {

class NewArrayInLoopFinder : public slang::syntax::SyntaxVisitor<NewArrayInLoopFinder> {
public:
    explicit NewArrayInLoopFinder(core::DiagnosticEngine& diags) : diags(diags) {}

    void handle(const slang::syntax::NewArrayExpressionSyntax& expr) {
        diags.addViolation(
            "SV-PRF-011",
            "DynamicArrayNewInLoop",
            "Dynamic array allocation 'new[...]' is invoked inside a simulation loop. "
            "In SystemVerilog performance optimization, allocating dynamic "
            "array heap storage inside simulation loops causes continuous heap memory churn, heap lock contention, and fragmentation. "
            "Pre-allocate array capacity prior to loop entry, or reuse an existing pre-allocated buffer/queue.",
            core::Severity::Warning,
            expr.sourceRange().start()
        );
        visitDefault(expr);
    }

private:
    core::DiagnosticEngine& diags;
};

class LoopArrayNewVisitor : public engine::SyntaxVisitor<LoopArrayNewVisitor> {
public:
    explicit LoopArrayNewVisitor(core::DiagnosticEngine& diags) : diags(diags) {}

    void handle(const slang::syntax::ForLoopStatementSyntax& stmt) {
        NewArrayInLoopFinder finder(diags);
        if (stmt.statement) stmt.statement->visit(finder);
        visitDefault(stmt);
    }

    void handle(const slang::syntax::ForeachLoopStatementSyntax& stmt) {
        NewArrayInLoopFinder finder(diags);
        if (stmt.statement) stmt.statement->visit(finder);
        visitDefault(stmt);
    }

    void handle(const slang::syntax::LoopStatementSyntax& stmt) {
        NewArrayInLoopFinder finder(diags);
        if (stmt.statement) stmt.statement->visit(finder);
        visitDefault(stmt);
    }

    void handle(const slang::syntax::DoWhileStatementSyntax& stmt) {
        NewArrayInLoopFinder finder(diags);
        if (stmt.statement) stmt.statement->visit(finder);
        visitDefault(stmt);
    }

    void handle(const slang::syntax::ForeverStatementSyntax& stmt) {
        NewArrayInLoopFinder finder(diags);
        if (stmt.statement) stmt.statement->visit(finder);
        visitDefault(stmt);
    }

private:
    core::DiagnosticEngine& diags;
};

class DynamicArrayNewInLoop : public Rule {
public:
    std::string_view getId() const override { return "SV-PRF-011"; }
    std::string_view getName() const override { return "DynamicArrayNewInLoop"; }
    std::string_view getDescription() const override {
        return "Disallows dynamic array allocation (new[...]) inside simulation loops to prevent heap contention and memory churn.";
    }
    std::string_view getCategory() const override { return "Performance"; }
    core::Severity getDefaultSeverity() const override { return core::Severity::Warning; }

    engine::RuleTask check(engine::RuleContext& ctx) override {
        auto& diags = ctx.diags();
        LoopArrayNewVisitor visitor(diags);
        co_await ctx.visitSyntax(visitor);
    }
};

} // namespace uvmcheck::rules::performance

namespace uvmcheck::rules {
std::unique_ptr<Rule> createDynamicArrayNewInLoop() {
    return std::make_unique<performance::DynamicArrayNewInLoop>();
}
} // namespace uvmcheck::rules
