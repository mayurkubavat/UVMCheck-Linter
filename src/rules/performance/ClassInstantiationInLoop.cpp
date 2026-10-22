#include "uvmcheck/rules/Rule.h"
#include "uvmcheck/rules/RuleRegistry.h"
#include "slang/syntax/AllSyntax.h"
#include "slang/syntax/SyntaxTree.h"
#include "slang/syntax/SyntaxVisitor.h"
#include <string>

namespace uvmcheck::rules::performance {

class NewInLoopFinder : public slang::syntax::SyntaxVisitor<NewInLoopFinder> {
public:
    explicit NewInLoopFinder(core::DiagnosticEngine& diags) : diags(diags) {}

    void handle(const slang::syntax::NewClassExpressionSyntax& expr) {
        diags.addViolation(
            "SV-OOP-010",
            "ClassInstantiationInLoop",
            "Direct object instantiation with 'new()' inside a simulation loop. Repeatedly creating dynamic heap objects in simulation loops triggers frequent garbage collection and memory manager lock contention. Preallocate objects outside the loop or reuse transaction instances from an object pool.",
            core::Severity::Warning,
            expr.scopedNew->getFirstToken().location()
        );
        visitDefault(expr);
    }

private:
    core::DiagnosticEngine& diags;
};

class LoopInstantiationVisitor : public engine::SyntaxVisitor<LoopInstantiationVisitor> {
public:
    explicit LoopInstantiationVisitor(core::DiagnosticEngine& diags) : diags(diags) {}

    void handle(const slang::syntax::ForLoopStatementSyntax& stmt) {
        NewInLoopFinder finder(diags);
        stmt.statement->visit(finder);
        visitDefault(stmt);
    }

    void handle(const slang::syntax::ForeachLoopStatementSyntax& stmt) {
        NewInLoopFinder finder(diags);
        stmt.statement->visit(finder);
        visitDefault(stmt);
    }

    void handle(const slang::syntax::LoopStatementSyntax& stmt) {
        NewInLoopFinder finder(diags);
        stmt.statement->visit(finder);
        visitDefault(stmt);
    }

    void handle(const slang::syntax::DoWhileStatementSyntax& stmt) {
        NewInLoopFinder finder(diags);
        stmt.statement->visit(finder);
        visitDefault(stmt);
    }

private:
    core::DiagnosticEngine& diags;
};

class ClassInstantiationInLoop : public Rule {
public:
    std::string_view getId() const override { return "SV-OOP-010"; }
    std::string_view getName() const override { return "ClassInstantiationInLoop"; }
    std::string_view getDescription() const override {
        return "Disallows direct class instantiation via new() inside simulation loops to prevent heap contention and memory allocation overhead.";
    }
    std::string_view getCategory() const override { return "Performance"; }
    core::Severity getDefaultSeverity() const override { return core::Severity::Warning; }

    engine::RuleTask check(engine::RuleContext& ctx) override {
        auto& diags = ctx.diags();
        LoopInstantiationVisitor visitor(diags);
        co_await ctx.visitSyntax(visitor);
    }
};

} // namespace uvmcheck::rules::performance

namespace uvmcheck::rules {
std::unique_ptr<Rule> createClassInstantiationInLoop() {
    return std::make_unique<performance::ClassInstantiationInLoop>();
}
} // namespace uvmcheck::rules
