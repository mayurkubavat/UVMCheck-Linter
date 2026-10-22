#include "uvmcheck/rules/Rule.h"
#include "uvmcheck/rules/RuleRegistry.h"
#include "slang/ast/ASTVisitor.h"
#include "slang/ast/symbols/SubroutineSymbols.h"
#include "slang/ast/statements/ConditionalStatements.h"
#include "slang/ast/statements/LoopStatements.h"
#include "slang/ast/expressions/OperatorExpressions.h"

namespace uvmcheck::rules::smells {

class ComplexityCounter : public slang::ast::ASTVisitor<ComplexityCounter, slang::ast::VisitFlags::AllGood> {
public:
    int complexity = 1;

    void handle(const slang::ast::ConditionalStatement& cond) {
        complexity++;
        visitDefault(cond);
    }

    void handle(const slang::ast::CaseStatement& caseStmt) {
        complexity += static_cast<int>(caseStmt.items.size());
        visitDefault(caseStmt);
    }

    void handle(const slang::ast::ForLoopStatement& loop) {
        complexity++;
        visitDefault(loop);
    }

    void handle(const slang::ast::ForeachLoopStatement& loop) {
        complexity++;
        visitDefault(loop);
    }

    void handle(const slang::ast::WhileLoopStatement& loop) {
        complexity++;
        visitDefault(loop);
    }

    void handle(const slang::ast::DoWhileLoopStatement& loop) {
        complexity++;
        visitDefault(loop);
    }

    void handle(const slang::ast::RepeatLoopStatement& loop) {
        complexity++;
        visitDefault(loop);
    }

    void handle(const slang::ast::ConditionalExpression& expr) {
        complexity++;
        visitDefault(expr);
    }

    void handle(const slang::ast::BinaryExpression& bin) {
        if (bin.op == slang::ast::BinaryOperator::LogicalAnd ||
            bin.op == slang::ast::BinaryOperator::LogicalOr) {
            complexity++;
        }
        visitDefault(bin);
    }
};

class CyclomaticVisitor : public engine::AstVisitor<CyclomaticVisitor> {
public:
    explicit CyclomaticVisitor(core::DiagnosticEngine& diags) : diags(diags) {}

    void checkSubroutine(const slang::ast::SubroutineSymbol& sub) {
        ComplexityCounter counter;
        sub.getBody().visit(counter);

        if (counter.complexity > 15) {
            core::Severity sev = (counter.complexity > 25) ? core::Severity::Warning : core::Severity::Info;
            diags.addViolation(
                "SV-SML-004",
                "CyclomaticComplexityGodMethod",
                "Subroutine '" + std::string(sub.name) +
                "' has high McCabe Cyclomatic Complexity (" + std::to_string(counter.complexity) +
                " > 15). Overly complex routines with deeply nested branching impede functional coverage, "
                "conceal unreachable edge cases, and lead to high defect density during maintenance. "
                "Decompose into smaller, modular sub-tasks.",
                sev,
                sub.location
            );
        }
    }

    void handle(const slang::ast::SubroutineSymbol& sub) {
        checkSubroutine(sub);
        visitDefault(sub);
    }

private:
    core::DiagnosticEngine& diags;
};

class CyclomaticComplexityGodMethod : public Rule {
public:
    std::string_view getId() const override { return "SV-SML-004"; }
    std::string_view getName() const override { return "CyclomaticComplexityGodMethod"; }
    std::string_view getDescription() const override {
        return "Detects subroutines with high McCabe Cyclomatic Complexity ($M > 15$). Complex control flow obscures edge cases and inhibits functional verification.";
    }
    std::string_view getCategory() const override { return "Smell"; }
    core::Severity getDefaultSeverity() const override { return core::Severity::Warning; }

    engine::RuleTask check(engine::RuleContext& ctx) override {
        auto& diags = ctx.diags();
        CyclomaticVisitor visitor(diags);
        co_await ctx.visitAst(visitor);
    }
};

} // namespace uvmcheck::rules::smells

namespace uvmcheck::rules {
std::unique_ptr<Rule> createCyclomaticComplexityGodMethod() {
    return std::make_unique<smells::CyclomaticComplexityGodMethod>();
}
}
