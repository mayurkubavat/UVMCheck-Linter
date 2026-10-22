#include "uvmcheck/rules/Rule.h"
#include "uvmcheck/rules/RuleRegistry.h"
#include "uvmcheck/core/ConfigManager.h"
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

class CyclomaticVisitor : public slang::ast::ASTVisitor<CyclomaticVisitor, slang::ast::VisitFlags::AllGood> {
public:
    explicit CyclomaticVisitor(core::DiagnosticEngine& diags) : diags(diags) {
        const auto& cfg = core::ConfigManager::instance().getProjectConfig();
        maxComplexity = cfg.ruleOptionInt("SV-SML-004", "max_complexity", 15);
        warningComplexity = cfg.ruleOptionInt("SV-SML-004", "warning_complexity", 25);
    }

    void checkSubroutine(const slang::ast::SubroutineSymbol& sub) {
        ComplexityCounter counter;
        sub.getBody().visit(counter);

        if (counter.complexity > maxComplexity) {
            core::Severity sev = (counter.complexity > warningComplexity) ? core::Severity::Warning : core::Severity::Info;
            diags.addViolation(
                "SV-SML-004",
                "CyclomaticComplexityGodMethod",
                "Subroutine '" + std::string(sub.name) +
                "' has high McCabe Cyclomatic Complexity (" + std::to_string(counter.complexity) +
                " > " + std::to_string(maxComplexity) + "). Overly complex routines with deeply nested branching impede functional coverage, "
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
    int64_t maxComplexity = 15;
    int64_t warningComplexity = 25;
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

    void run(slang::ast::Compilation& compilation,
             const uvmcheck::uvm::UvmModel&,
             core::DiagnosticEngine& diags) override {
        CyclomaticVisitor visitor(diags);
        compilation.getRoot().visit(visitor);
    }
};

} // namespace uvmcheck::rules::smells

namespace uvmcheck::rules {
std::unique_ptr<Rule> createCyclomaticComplexityGodMethod() {
    return std::make_unique<smells::CyclomaticComplexityGodMethod>();
}
}
