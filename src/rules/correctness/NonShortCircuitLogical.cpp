#include "uvmcheck/rules/Rule.h"
#include "uvmcheck/rules/RuleRegistry.h"
#include "slang/syntax/AllSyntax.h"
#include "slang/syntax/SyntaxTree.h"
#include "slang/syntax/SyntaxVisitor.h"
#include <string>

namespace uvmcheck::rules::correctness {

class NonShortCircuitVisitor : public slang::syntax::SyntaxVisitor<NonShortCircuitVisitor> {
public:
    explicit NonShortCircuitVisitor(core::DiagnosticEngine& diags) : diags(diags) {}

    void handle(const slang::syntax::ConditionalPredicateSyntax& pred) {
        for (const auto* cond : pred.conditions) {
            if (cond->expr) {
                checkExpr(*cond->expr);
            }
        }
        visitDefault(pred);
    }

private:
    core::DiagnosticEngine& diags;

    static bool isComparisonOrLogical(slang::syntax::SyntaxKind kind) {
        switch (kind) {
            case slang::syntax::SyntaxKind::EqualityExpression:
            case slang::syntax::SyntaxKind::InequalityExpression:
            case slang::syntax::SyntaxKind::CaseEqualityExpression:
            case slang::syntax::SyntaxKind::CaseInequalityExpression:
            case slang::syntax::SyntaxKind::WildcardEqualityExpression:
            case slang::syntax::SyntaxKind::WildcardInequalityExpression:
            case slang::syntax::SyntaxKind::LessThanExpression:
            case slang::syntax::SyntaxKind::LessThanEqualExpression:
            case slang::syntax::SyntaxKind::GreaterThanExpression:
            case slang::syntax::SyntaxKind::GreaterThanEqualExpression:
            case slang::syntax::SyntaxKind::LogicalAndExpression:
            case slang::syntax::SyntaxKind::LogicalOrExpression:
            case slang::syntax::SyntaxKind::LogicalImplicationExpression:
            case slang::syntax::SyntaxKind::LogicalEquivalenceExpression:
                return true;
            default:
                return false;
        }
    }

    void checkExpr(const slang::syntax::ExpressionSyntax& expr) {
        if (expr.kind == slang::syntax::SyntaxKind::BinaryAndExpression ||
            expr.kind == slang::syntax::SyntaxKind::BinaryOrExpression) {
            const auto& binExpr = expr.as<slang::syntax::BinaryExpressionSyntax>();
            std::string op = std::string(binExpr.operatorToken.valueText());

            // Check if either child is a comparison/relational expression, indicating boolean intent
            bool isBooleanContext = isComparisonOrLogical(binExpr.left->kind) ||
                                   isComparisonOrLogical(binExpr.right->kind);

            // Also check parenthesized comparison expressions: (a != null) & (b != null)
            if (!isBooleanContext) {
                if (binExpr.left->kind == slang::syntax::SyntaxKind::ParenthesizedExpression) {
                    const auto& paren = binExpr.left->as<slang::syntax::ParenthesizedExpressionSyntax>();
                    if (paren.expression && isComparisonOrLogical(paren.expression->kind)) {
                        isBooleanContext = true;
                    }
                }
                if (binExpr.right->kind == slang::syntax::SyntaxKind::ParenthesizedExpression) {
                    const auto& paren = binExpr.right->as<slang::syntax::ParenthesizedExpressionSyntax>();
                    if (paren.expression && isComparisonOrLogical(paren.expression->kind)) {
                        isBooleanContext = true;
                    }
                }
            }

            if (isBooleanContext && (op == "&" || op == "|")) {
                std::string replacement = (op == "&") ? "&&" : "||";
                std::vector<core::FixItHint> fixIts;
                if (binExpr.operatorToken.range().start().valid()) {
                    fixIts.push_back({
                        binExpr.operatorToken.range(),
                        replacement,
                        "Replace non-short-circuiting bitwise '" + op + "' with logical '" + replacement + "'"
                    });
                }

                diags.addViolation(
                    "SV-COR-005",
                    "NonShortCircuitLogical",
                    "Bitwise operator '" + op + "' used in conditional predicate where logical '" +
                    replacement + "' is expected. Bitwise operators do not short-circuit, unconditionally evaluating the right-hand side and risking null-pointer dereferences or index-out-of-bounds faults. Replace with '" +
                    replacement + "'.",
                    core::Severity::Warning,
                    binExpr.operatorToken.location(),
                    fixIts
                );
            }

            checkExpr(*binExpr.left);
            checkExpr(*binExpr.right);
            return;
        }

        if (expr.kind == slang::syntax::SyntaxKind::ParenthesizedExpression) {
            const auto& paren = expr.as<slang::syntax::ParenthesizedExpressionSyntax>();
            if (paren.expression) {
                checkExpr(*paren.expression);
            }
        }
    }
};

class NonShortCircuitLogical : public Rule {
public:
    std::string_view getId() const override { return "SV-COR-005"; }
    std::string_view getName() const override { return "NonShortCircuitLogical"; }
    std::string_view getDescription() const override {
        return "Disallows using bitwise AND ('&') or bitwise OR ('|') instead of logical short-circuiting operators ('&&', '||') inside conditional statements.";
    }
    std::string_view getCategory() const override { return "Correctness"; }
    core::Severity getDefaultSeverity() const override { return core::Severity::Warning; }

    void run(slang::ast::Compilation& compilation,
             const uvmcheck::uvm::UvmModel& /*uvmModel*/,
             core::DiagnosticEngine& diags) override {
        NonShortCircuitVisitor visitor(diags);
        for (const auto& tree : compilation.getSyntaxTrees()) {
            if (tree) {
                tree->root().visit(visitor);
            }
        }
    }
};

} // namespace uvmcheck::rules::correctness

namespace uvmcheck::rules {
std::unique_ptr<Rule> createNonShortCircuitLogical() {
    return std::make_unique<correctness::NonShortCircuitLogical>();
}
}
