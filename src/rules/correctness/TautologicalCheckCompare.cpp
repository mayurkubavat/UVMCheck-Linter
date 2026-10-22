#include "uvmcheck/rules/Rule.h"
#include "uvmcheck/rules/RuleRegistry.h"
#include "slang/ast/ASTVisitor.h"
#include "slang/ast/expressions/OperatorExpressions.h"
#include "slang/ast/expressions/ConversionExpression.h"
#include "slang/ast/expressions/LiteralExpressions.h"
#include "slang/syntax/AllSyntax.h"

namespace uvmcheck::rules::correctness {

class TautologicalCompareVisitor : public engine::AstVisitor<TautologicalCompareVisitor> {
public:
    explicit TautologicalCompareVisitor(core::DiagnosticEngine& diags) : diags(diags) {}

    void handle(const slang::ast::BinaryExpression& expr) {
        switch (expr.op) {
            case slang::ast::BinaryOperator::Equality:
            case slang::ast::BinaryOperator::Inequality:
            case slang::ast::BinaryOperator::CaseEquality:
            case slang::ast::BinaryOperator::CaseInequality:
            case slang::ast::BinaryOperator::WildcardEquality:
            case slang::ast::BinaryOperator::WildcardInequality:
            case slang::ast::BinaryOperator::LessThan:
            case slang::ast::BinaryOperator::LessThanEqual:
            case slang::ast::BinaryOperator::GreaterThan:
            case slang::ast::BinaryOperator::GreaterThanEqual: {
                const auto* left = &expr.left();
                while (left->kind == slang::ast::ExpressionKind::Conversion) {
                    left = &left->as<slang::ast::ConversionExpression>().operand();
                }

                const auto* right = &expr.right();
                while (right->kind == slang::ast::ExpressionKind::Conversion) {
                    right = &right->as<slang::ast::ConversionExpression>().operand();
                }

                // Skip pure numeric literals like 1 == 1 often found in macro stubs
                if (left->kind == slang::ast::ExpressionKind::IntegerLiteral ||
                    left->kind == slang::ast::ExpressionKind::RealLiteral ||
                    right->kind == slang::ast::ExpressionKind::IntegerLiteral ||
                    right->kind == slang::ast::ExpressionKind::RealLiteral) {
                    break;
                }

                if (left->syntax && right->syntax) {
                    std::string leftStr = trim(std::string(left->syntax->toString()));
                    std::string rightStr = trim(std::string(right->syntax->toString()));

                    if (!leftStr.empty() && leftStr == rightStr) {
                        // Ensure it references identifiers rather than purely operator tokens
                        bool hasAlpha = false;
                        for (char c : leftStr) {
                            if (std::isalpha(static_cast<unsigned char>(c)) || c == '_') {
                                hasAlpha = true;
                                break;
                            }
                        }

                        if (hasAlpha) {
                            diags.addViolation(
                                "SV-COR-001",
                                "TautologicalCheckCompare",
                                "Tautological comparison between identical expressions '" + leftStr + "' and '" + rightStr + "'. "
                                "In verification checks and scoreboards, comparing an expression to itself always evaluates to a constant "
                                "result, masking functional RTL bugs and creating silent false-positive test results.",
                                core::Severity::Error,
                                expr.sourceRange.start()
                            );
                        }
                    }
                }
                break;
            }
            default:
                break;
        }
        visitDefault(expr);
    }

private:
    static std::string trim(const std::string& str) {
        size_t first = str.find_first_not_of(" \t\r\n");
        if (first == std::string::npos) return "";
        size_t last = str.find_last_not_of(" \t\r\n");
        return str.substr(first, (last - first + 1));
    }

    core::DiagnosticEngine& diags;
};

class TautologicalCheckCompare : public Rule {
public:
    std::string_view getId() const override { return "SV-COR-001"; }
    std::string_view getName() const override { return "TautologicalCheckCompare"; }
    std::string_view getDescription() const override {
        return "Detects tautological comparison between identical expressions (e.g. 'act_pkt.data == act_pkt.data'). "
               "Self-comparisons in scoreboards and checks always evaluate to true, masking hardware bugs.";
    }
    std::string_view getCategory() const override { return "Correctness"; }
    core::Severity getDefaultSeverity() const override { return core::Severity::Error; }

    engine::RuleTask check(engine::RuleContext& ctx) override {
        auto& diags = ctx.diags();
        TautologicalCompareVisitor visitor(diags);
        co_await ctx.visitAst(visitor);
    }
};

} // namespace uvmcheck::rules::correctness

namespace uvmcheck::rules {
std::unique_ptr<Rule> createTautologicalCheckCompare() {
    return std::make_unique<correctness::TautologicalCheckCompare>();
}
}
