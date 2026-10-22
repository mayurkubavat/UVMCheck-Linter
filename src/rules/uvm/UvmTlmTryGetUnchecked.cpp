#include "uvmcheck/rules/Rule.h"
#include "uvmcheck/rules/RuleRegistry.h"
#include "slang/syntax/AllSyntax.h"
#include "slang/syntax/SyntaxTree.h"
#include "slang/syntax/SyntaxVisitor.h"
#include <string>

namespace uvmcheck::rules::uvm {

class TlmTryGetVisitor : public engine::SyntaxVisitor<TlmTryGetVisitor> {
public:
    explicit TlmTryGetVisitor(core::DiagnosticEngine& diags) : diags(diags) {}

    void handle(const slang::syntax::ExpressionStatementSyntax& stmt) {
        checkExpression(stmt.expr);
        visitDefault(stmt);
    }

private:
    core::DiagnosticEngine& diags;

    void checkExpression(const slang::syntax::ExpressionSyntax* expr) {
        if (!expr) return;

        const slang::syntax::InvocationExpressionSyntax* inv = nullptr;
        if (expr->kind == slang::syntax::SyntaxKind::InvocationExpression) {
            inv = &expr->as<slang::syntax::InvocationExpressionSyntax>();
        } else if (expr->kind == slang::syntax::SyntaxKind::CastExpression) {
            const auto& cast = expr->as<slang::syntax::CastExpressionSyntax>();
            if (cast.right && cast.right->expression &&
                cast.right->expression->kind == slang::syntax::SyntaxKind::InvocationExpression) {
                inv = &cast.right->expression->as<slang::syntax::InvocationExpressionSyntax>();
            }
        }

        if (!inv || !inv->left) return;

        std::string methodName = std::string(inv->left->getLastToken().valueText());
        if (methodName == "try_get" || methodName == "try_peek" || methodName == "try_put") {
            diags.addViolation(
                "UVM-TLM-002",
                "UvmTlmTryGetUnchecked",
                "Calling non-blocking TLM method '." + methodName +
                "()' without checking its return boolean status causes silent transaction drops when the channel is empty or full. Check the return status in an 'if' statement (e.g. 'if (!port." +
                methodName + "(tr)) ...').",
                core::Severity::Warning,
                inv->getFirstToken().location()
            );
        }
    }
};

class UvmTlmTryGetUnchecked : public Rule {
public:
    std::string_view getId() const override { return "UVM-TLM-002"; }
    std::string_view getName() const override { return "UvmTlmTryGetUnchecked"; }
    std::string_view getDescription() const override {
        return "Flags calls to non-blocking TLM methods (try_get(), try_peek(), try_put()) where the boolean return status is ignored.";
    }
    std::string_view getCategory() const override { return "UVM"; }
    core::Severity getDefaultSeverity() const override { return core::Severity::Warning; }

    engine::RuleTask check(engine::RuleContext& ctx) override {
        auto& diags = ctx.diags();
        TlmTryGetVisitor visitor(diags);
        co_await ctx.visitSyntax(visitor);
    }
};

} // namespace uvmcheck::rules::uvm

namespace uvmcheck::rules {
std::unique_ptr<Rule> createUvmTlmTryGetUnchecked() {
    return std::make_unique<uvm::UvmTlmTryGetUnchecked>();
}
}
