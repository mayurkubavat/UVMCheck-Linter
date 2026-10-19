#include "uvmcheck/rules/Rule.h"
#include "uvmcheck/rules/RuleRegistry.h"
#include "slang/ast/ASTVisitor.h"
#include "slang/ast/expressions/OperatorExpressions.h"
#include "slang/ast/expressions/LiteralExpressions.h"
#include "slang/ast/types/Type.h"

#include "slang/ast/expressions/ConversionExpression.h"

namespace uvmcheck::rules::types {

class MixedSignVisitor : public slang::ast::ASTVisitor<MixedSignVisitor, slang::ast::VisitFlags::AllGood> {
public:
    explicit MixedSignVisitor(core::DiagnosticEngine& diags) : diags(diags) {}

    void handle(const slang::ast::BinaryExpression& expr) {
        switch (expr.op) {
            case slang::ast::BinaryOperator::LessThan:
            case slang::ast::BinaryOperator::LessThanEqual:
            case slang::ast::BinaryOperator::GreaterThan:
            case slang::ast::BinaryOperator::GreaterThanEqual:
            case slang::ast::BinaryOperator::Equality:
            case slang::ast::BinaryOperator::Inequality:
            case slang::ast::BinaryOperator::CaseEquality:
            case slang::ast::BinaryOperator::CaseInequality: {
                const auto* left = &expr.left();
                while (left->kind == slang::ast::ExpressionKind::Conversion) {
                    left = &left->as<slang::ast::ConversionExpression>().operand();
                }

                const auto* right = &expr.right();
                while (right->kind == slang::ast::ExpressionKind::Conversion) {
                    right = &right->as<slang::ast::ConversionExpression>().operand();
                }

                const auto& leftType = *left->type;
                const auto& rightType = *right->type;

                if (leftType.isIntegral() && rightType.isIntegral()) {
                    bool leftSigned = leftType.isSigned();
                    bool rightSigned = rightType.isSigned();

                    if (leftSigned != rightSigned) {
                        // Avoid false positives on comparisons against simple non-negative literal constants like '0'
                        bool leftIsNonNegLiteral = false;
                        if (left->kind == slang::ast::ExpressionKind::IntegerLiteral) {
                            if (!left->as<slang::ast::IntegerLiteral>().getValue().isNegative()) {
                                leftIsNonNegLiteral = true;
                            }
                        }

                        bool rightIsNonNegLiteral = false;
                        if (right->kind == slang::ast::ExpressionKind::IntegerLiteral) {
                            if (!right->as<slang::ast::IntegerLiteral>().getValue().isNegative()) {
                                rightIsNonNegLiteral = true;
                            }
                        }

                        // Flag if either side is a signed non-literal, or is explicitly negative
                        if ((leftSigned && !leftIsNonNegLiteral) || (rightSigned && !rightIsNonNegLiteral)) {
                            std::string signedSide = leftSigned ? "LHS" : "RHS";
                            std::string leftDesc = std::string(leftType.toString());
                            std::string rightDesc = std::string(rightType.toString());

                            diags.addViolation(
                                "SV-TYP-001",
                                "MixedSignComparison",
                                "Comparison between signed type '" + leftDesc + "' and unsigned type '" +
                                rightDesc + "'. In SystemVerilog (IEEE 1800 Sec 11.8.2), if either operand is unsigned, "
                                "the entire comparison is coerced to unsigned, causing negative numbers on the " +
                                signedSide + " to evaluate as large positive numbers.",
                                core::Severity::Warning,
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
    core::DiagnosticEngine& diags;
};

class MixedSignComparison : public Rule {
public:
    std::string_view getId() const override { return "SV-TYP-001"; }
    std::string_view getName() const override { return "MixedSignComparison"; }
    std::string_view getDescription() const override {
        return "Comparing signed and unsigned integers causes implicit unsigned coercion of the signed operand. "
               "Negative values evaluate as huge positive numbers, causing comparisons to silently invert logic.";
    }
    std::string_view getCategory() const override { return "Arithmetic"; }
    core::Severity getDefaultSeverity() const override { return core::Severity::Warning; }

    void run(slang::ast::Compilation& compilation,
             const uvmcheck::uvm::UvmModel& /*uvmModel*/,
             core::DiagnosticEngine& diags) override {
        MixedSignVisitor visitor(diags);
        compilation.getRoot().visit(visitor);
    }
};

} // namespace uvmcheck::rules::types

namespace uvmcheck::rules {
std::unique_ptr<Rule> createMixedSignComparison() {
    return std::make_unique<types::MixedSignComparison>();
}
}
