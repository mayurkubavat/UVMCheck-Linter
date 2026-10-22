#include "uvmcheck/rules/Rule.h"
#include "uvmcheck/rules/RuleRegistry.h"
#include "slang/syntax/AllSyntax.h"
#include "slang/syntax/SyntaxTree.h"
#include "slang/syntax/SyntaxVisitor.h"
#include <string>

namespace uvmcheck::rules::correctness {

class AssignmentFinder : public slang::syntax::SyntaxVisitor<AssignmentFinder> {
public:
    explicit AssignmentFinder(core::DiagnosticEngine& diags) : diags(diags) {}

    void handle(const slang::syntax::BinaryExpressionSyntax& expr) {
        if (expr.kind == slang::syntax::SyntaxKind::AssignmentExpression ||
            expr.kind == slang::syntax::SyntaxKind::AddAssignmentExpression ||
            expr.kind == slang::syntax::SyntaxKind::SubtractAssignmentExpression ||
            expr.kind == slang::syntax::SyntaxKind::MultiplyAssignmentExpression ||
            expr.kind == slang::syntax::SyntaxKind::DivideAssignmentExpression ||
            expr.kind == slang::syntax::SyntaxKind::NonblockingAssignmentExpression) {

            std::string opStr = std::string(expr.operatorToken.valueText());
            std::vector<core::FixItHint> fixIts;

            if (expr.operatorToken.kind == slang::parsing::TokenKind::Equals) {
                fixIts.push_back({
                    slang::SourceRange(expr.operatorToken.location(),
                                       expr.operatorToken.location() + expr.operatorToken.rawText().length()),
                    "==",
                    "Replace assignment '=' with equality operator '=='"
                });
            }

            diags.addViolation(
                "SV-COR-007",
                "AssignmentInExpression",
                "Assignment operator '" + opStr +
                "' used inside conditional predicate. In SystemVerilog, an assignment assigns the value and evaluates its truthiness, which is frequently an unintended typo for the equality operator '=='.",
                core::Severity::Warning,
                expr.operatorToken.location(),
                fixIts
            );
        }

        visitDefault(expr);
    }

private:
    core::DiagnosticEngine& diags;
};

class ConditionVisitor : public engine::SyntaxVisitor<ConditionVisitor> {
public:
    explicit ConditionVisitor(core::DiagnosticEngine& diags) : diags(diags) {}

    void handle(const slang::syntax::ConditionalStatementSyntax& stmt) {
        if (stmt.predicate) {
            for (const auto* cond : stmt.predicate->conditions) {
                if (cond && cond->expr) {
                    AssignmentFinder finder(diags);
                    cond->expr->visit(finder);
                }
            }
        }
        visitDefault(stmt);
    }

    void handle(const slang::syntax::ConditionalExpressionSyntax& expr) {
        if (expr.predicate) {
            AssignmentFinder finder(diags);
            expr.predicate->visit(finder);
        }
        visitDefault(expr);
    }

    void handle(const slang::syntax::LoopStatementSyntax& stmt) {
        if (stmt.expr) {
            AssignmentFinder finder(diags);
            stmt.expr->visit(finder);
        }
        visitDefault(stmt);
    }

    void handle(const slang::syntax::DoWhileStatementSyntax& stmt) {
        if (stmt.expr) {
            AssignmentFinder finder(diags);
            stmt.expr->visit(finder);
        }
        visitDefault(stmt);
    }

    void handle(const slang::syntax::ForLoopStatementSyntax& stmt) {
        if (stmt.stopExpr) {
            AssignmentFinder finder(diags);
            stmt.stopExpr->visit(finder);
        }
        visitDefault(stmt);
    }

private:
    core::DiagnosticEngine& diags;
};

class AssignmentInExpression : public Rule {
public:
    std::string_view getId() const override { return "SV-COR-007"; }
    std::string_view getName() const override { return "AssignmentInExpression"; }
    std::string_view getDescription() const override {
        return "Disallows assignment expressions ('=' or compound assignments) inside conditional predicates or loop conditions.";
    }
    std::string_view getCategory() const override { return "Correctness"; }
    core::Severity getDefaultSeverity() const override { return core::Severity::Warning; }

    engine::RuleTask check(engine::RuleContext& ctx) override {
        auto& diags = ctx.diags();
        ConditionVisitor visitor(diags);
        co_await ctx.visitSyntax(visitor);
    }
};

} // namespace uvmcheck::rules::correctness

namespace uvmcheck::rules {
std::unique_ptr<Rule> createAssignmentInExpression() {
    return std::make_unique<correctness::AssignmentInExpression>();
}
} // namespace uvmcheck::rules
