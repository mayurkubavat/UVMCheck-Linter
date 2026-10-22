#include "uvmcheck/rules/Rule.h"
#include "uvmcheck/rules/RuleRegistry.h"
#include "slang/ast/ASTVisitor.h"
#include "slang/ast/expressions/Operator.h"
#include "slang/ast/expressions/LiteralExpressions.h"
#include "slang/syntax/AllSyntax.h"

namespace uvmcheck::rules::correctness {

class EqualityWithXzVisitor : public engine::AstVisitor<EqualityWithXzVisitor> {
public:
    explicit EqualityWithXzVisitor(core::DiagnosticEngine& diags) : diags(diags) {}

    void handle(const slang::ast::BinaryExpression& expr) {
        if (expr.op == slang::ast::BinaryOperator::Equality ||
            expr.op == slang::ast::BinaryOperator::Inequality) {

            bool leftUnknown = isUnknownLiteral(expr.left());
            bool rightUnknown = isUnknownLiteral(expr.right());

            if (leftUnknown || rightUnknown) {
                std::string opStr = (expr.op == slang::ast::BinaryOperator::Equality) ? "==" : "!=";
                std::string caseOpStr = (expr.op == slang::ast::BinaryOperator::Equality) ? "===" : "!==";

                std::vector<core::FixItHint> fixIts;
                if (expr.syntax &&
                    (expr.syntax->kind == slang::syntax::SyntaxKind::EqualityExpression ||
                     expr.syntax->kind == slang::syntax::SyntaxKind::InequalityExpression)) {
                    const auto& binSyntax = expr.syntax->as<slang::syntax::BinaryExpressionSyntax>();
                    if (binSyntax.operatorToken.range().start().valid()) {
                        fixIts.push_back({
                            binSyntax.operatorToken.range(),
                            caseOpStr,
                            "Replace '" + opStr + "' with case equality '" + caseOpStr + "'"
                        });
                    }
                }

                diags.addViolation(
                    "SV-LANG-001",
                    "EqualityWithXz",
                    "Dangerous 4-state equality check '" + opStr + "' against unknown literal (x/z). "
                    "In SystemVerilog (IEEE 1800 § 11.4.5), logical equality ('==', '!=') against 'x' or 'z' evaluates to 'x' "
                    "(which evaluates to false in conditionals), silently masking verification failures. "
                    "Use case equality ('" + caseOpStr + "') or wildcard equality ('==?', '!=?').",
                    core::Severity::Error,
                    expr.sourceRange.start(),
                    std::move(fixIts)
                );
            }
        }
        visitDefault(expr);
    }

private:
    static bool isUnknownLiteral(const slang::ast::Expression& expr) {
        if (expr.kind == slang::ast::ExpressionKind::IntegerLiteral) {
            return expr.as<slang::ast::IntegerLiteral>().getValue().hasUnknown();
        }
        if (expr.kind == slang::ast::ExpressionKind::UnbasedUnsizedIntegerLiteral) {
            return expr.as<slang::ast::UnbasedUnsizedIntegerLiteral>().getValue().hasUnknown();
        }
        return false;
    }

    core::DiagnosticEngine& diags;
};

class EqualityWithXz : public Rule {
public:
    std::string_view getId() const override { return "SV-LANG-001"; }
    std::string_view getName() const override { return "EqualityWithXz"; }
    std::string_view getDescription() const override {
        return "Detects equality ('==', '!=') checks against 4-state unknown literals ('x, 'z). "
               "Logical equality against unknowns evaluates to 'x (false in branch conditions), silently masking bugs. "
               "Use case equality ('===', '!==') instead.";
    }
    std::string_view getCategory() const override { return "Correctness"; }
    core::Severity getDefaultSeverity() const override { return core::Severity::Error; }

    engine::RuleTask check(engine::RuleContext& ctx) override {
        auto& diags = ctx.diags();
        EqualityWithXzVisitor visitor(diags);
        co_await ctx.visitAst(visitor);
    }
};

} // namespace uvmcheck::rules::correctness

namespace uvmcheck::rules {
std::unique_ptr<Rule> createEqualityWithXz() {
    return std::make_unique<correctness::EqualityWithXz>();
}
}
