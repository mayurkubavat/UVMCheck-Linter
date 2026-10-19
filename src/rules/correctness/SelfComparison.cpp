#include "uvmcheck/rules/Rule.h"
#include "uvmcheck/rules/RuleRegistry.h"
#include "slang/syntax/AllSyntax.h"
#include "slang/syntax/SyntaxTree.h"
#include "slang/syntax/SyntaxVisitor.h"
#include <string>

namespace uvmcheck::rules::correctness {

class SelfComparisonVisitor : public slang::syntax::SyntaxVisitor<SelfComparisonVisitor> {
public:
    explicit SelfComparisonVisitor(core::DiagnosticEngine& diags) : diags(diags) {}

    void handle(const slang::syntax::BinaryExpressionSyntax& expr) {
        switch (expr.kind) {
            case slang::syntax::SyntaxKind::EqualityExpression:
            case slang::syntax::SyntaxKind::InequalityExpression:
            case slang::syntax::SyntaxKind::CaseEqualityExpression:
            case slang::syntax::SyntaxKind::CaseInequalityExpression:
            case slang::syntax::SyntaxKind::WildcardEqualityExpression:
            case slang::syntax::SyntaxKind::WildcardInequalityExpression:
            case slang::syntax::SyntaxKind::LessThanExpression:
            case slang::syntax::SyntaxKind::LessThanEqualExpression:
            case slang::syntax::SyntaxKind::GreaterThanExpression:
            case slang::syntax::SyntaxKind::GreaterThanEqualExpression: {
                if (expr.left && expr.right) {
                    std::string leftStr = cleanExpr(expr.left);
                    std::string rightStr = cleanExpr(expr.right);

                    if (!leftStr.empty() && leftStr == rightStr) {
                        std::string opStr = std::string(expr.operatorToken.valueText());
                        diags.addViolation(
                            "SV-COR-008",
                            "SelfComparison",
                            "Self-comparison detected: expression '" + leftStr + "' is compared to itself using operator '" +
                            opStr + "'. Self-comparisons tautologically evaluate to constant true or false, almost invariably indicating a copy-paste error where another variable or expected value was intended.",
                            core::Severity::Error,
                            expr.operatorToken.location()
                        );
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
    static std::string cleanExpr(const slang::syntax::ExpressionSyntax* expr) {
        if (!expr) return "";
        std::string s = std::string(expr->toString());
        s.erase(0, s.find_first_not_of(" \t\n\r"));
        if (size_t end = s.find_last_not_of(" \t\n\r"); end != std::string::npos) {
            s.erase(end + 1);
        }
        return s;
    }

    core::DiagnosticEngine& diags;
};

class SelfComparison : public Rule {
public:
    std::string_view getId() const override { return "SV-COR-008"; }
    std::string_view getName() const override { return "SelfComparison"; }
    std::string_view getDescription() const override {
        return "Disallows self-comparisons where an operand is compared to itself (e.g. a == a), which tautologically evaluates to constant true or false.";
    }
    std::string_view getCategory() const override { return "Correctness"; }
    core::Severity getDefaultSeverity() const override { return core::Severity::Error; }

    void run(slang::ast::Compilation& compilation,
             const uvmcheck::uvm::UvmModel& /*uvmModel*/,
             core::DiagnosticEngine& diags) override {
        SelfComparisonVisitor visitor(diags);
        for (const auto& tree : compilation.getSyntaxTrees()) {
            if (tree) {
                tree->root().visit(visitor);
            }
        }
    }
};

} // namespace uvmcheck::rules::correctness

namespace uvmcheck::rules {
std::unique_ptr<Rule> createSelfComparison() {
    return std::make_unique<correctness::SelfComparison>();
}
} // namespace uvmcheck::rules
