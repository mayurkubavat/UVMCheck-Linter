#include "uvmcheck/rules/Rule.h"
#include "uvmcheck/rules/RuleRegistry.h"
#include "slang/ast/ASTVisitor.h"
#include "slang/ast/statements/MiscStatements.h"

namespace uvmcheck::rules::sva {

class TimingControlVisitor : public slang::ast::ASTVisitor<TimingControlVisitor, slang::ast::VisitFlags::AllGood> {
public:
    void handle(const slang::ast::TimedStatement& stmt) {
        hasTiming = true;
        firstLoc = stmt.sourceRange.start();
        visitDefault(stmt);
    }

    void handle(const slang::ast::WaitStatement& stmt) {
        hasTiming = true;
        firstLoc = stmt.sourceRange.start();
        visitDefault(stmt);
    }

    bool foundTiming() const { return hasTiming; }
    slang::SourceLocation getLocation() const { return firstLoc; }

private:
    bool hasTiming = false;
    slang::SourceLocation firstLoc;
};

class TimeInAssertionActionVisitor : public engine::AstVisitor<TimeInAssertionActionVisitor> {
public:
    explicit TimeInAssertionActionVisitor(core::DiagnosticEngine& diags) : diags(diags) {}

    void checkAction(const slang::ast::Statement* stmt, std::string_view kindStr) {
        if (!stmt) return;
        TimingControlVisitor visitor;
        stmt->visit(visitor);
        if (visitor.foundTiming()) {
            diags.addViolation(
                "SVA-ACT-002",
                "TimeInAssertionAction",
                "Time-consuming statement (#delay, @event, wait) detected inside " + std::string(kindStr) +
                " assertion action block. In SystemVerilog (IEEE 1800 § 16), assertion action blocks execute in the Reactive region and must not consume simulation time.",
                core::Severity::Error,
                visitor.getLocation()
            );
        }
    }

    void handle(const slang::ast::ImmediateAssertionStatement& stmt) {
        checkAction(stmt.ifTrue, "immediate");
        checkAction(stmt.ifFalse, "immediate");
        visitDefault(stmt);
    }

    void handle(const slang::ast::ConcurrentAssertionStatement& stmt) {
        checkAction(stmt.ifTrue, "concurrent");
        checkAction(stmt.ifFalse, "concurrent");
        visitDefault(stmt);
    }

private:
    core::DiagnosticEngine& diags;
};

class TimeInAssertionAction : public Rule {
public:
    std::string_view getId() const override { return "SVA-ACT-002"; }
    std::string_view getName() const override { return "TimeInAssertionAction"; }
    std::string_view getDescription() const override {
        return "Action blocks of immediate and concurrent assertions must not consume simulation time "
               "(# delays, @ event controls, or wait statements) per IEEE 1800 § 16.";
    }
    std::string_view getCategory() const override { return "SVA"; }
    core::Severity getDefaultSeverity() const override { return core::Severity::Error; }

    engine::RuleTask check(engine::RuleContext& ctx) override {
        auto& diags = ctx.diags();
        TimeInAssertionActionVisitor visitor(diags);
        co_await ctx.visitAst(visitor);
    }
};

} // namespace uvmcheck::rules::sva

namespace uvmcheck::rules {
std::unique_ptr<Rule> createTimeInAssertionAction() {
    return std::make_unique<sva::TimeInAssertionAction>();
}
}
