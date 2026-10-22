#include "uvmcheck/rules/Rule.h"
#include "uvmcheck/rules/RuleRegistry.h"
#include "slang/syntax/AllSyntax.h"
#include "slang/syntax/SyntaxTree.h"
#include "slang/syntax/SyntaxVisitor.h"
#include <string>

namespace uvmcheck::rules::sva {

static bool isSimpleResetExpression(const slang::syntax::ExpressionSyntax& expr) {
    if (expr.kind == slang::syntax::SyntaxKind::IdentifierName) {
        return true;
    }
    if (expr.kind == slang::syntax::SyntaxKind::UnaryLogicalNotExpression ||
        expr.kind == slang::syntax::SyntaxKind::UnaryBitwiseNotExpression) {
        const auto& un = expr.as<slang::syntax::PrefixUnaryExpressionSyntax>();
        if (un.operand && un.operand->kind == slang::syntax::SyntaxKind::IdentifierName) {
            return true;
        }
    }
    if (expr.kind == slang::syntax::SyntaxKind::ParenthesizedExpression) {
        const auto& paren = expr.as<slang::syntax::ParenthesizedExpressionSyntax>();
        if (paren.expression) {
            return isSimpleResetExpression(*paren.expression);
        }
    }
    if (expr.kind == slang::syntax::SyntaxKind::EqualityExpression ||
        expr.kind == slang::syntax::SyntaxKind::InequalityExpression) {
        const auto& bin = expr.as<slang::syntax::BinaryExpressionSyntax>();
        if (bin.left && bin.right) {
            bool leftSimple = (bin.left->kind == slang::syntax::SyntaxKind::IdentifierName);
            bool rightLit = (bin.right->kind == slang::syntax::SyntaxKind::IntegerLiteralExpression ||
                             bin.right->kind == slang::syntax::SyntaxKind::IntegerVectorExpression);
            if (leftSimple && rightLit) return true;
        }
    }
    return false;
}

class DisableIffComplexVisitor : public engine::SyntaxVisitor<DisableIffComplexVisitor> {
public:
    explicit DisableIffComplexVisitor(core::DiagnosticEngine& diags) : diags(diags) {}

    void checkPropertySpec(const slang::syntax::PropertySpecSyntax* spec,
                           slang::SourceLocation fallbackLoc) {
        if (!spec || !spec->disable) return;

        const auto* dis = spec->disable;
        if (dis && dis->expr) {
            if (!isSimpleResetExpression(*dis->expr)) {
                std::string exprText = std::string(dis->expr->toString());
                diags.addViolation(
                    "SV-SVA-002",
                    "DisableIffComplexExpression",
                    "'disable iff (...)' clause contains complex expression '" + exprText + "'. "
                    "In SystemVerilog assertion guidelines (IEEE 1800 § 16.12), "
                    "'disable iff' evaluates asynchronously on every signal glitch and should be strictly restricted to "
                    "simple reset signals (e.g. '!rst_n') to avoid glitch sensitivity and formal property state explosion.",
                    core::Severity::Warning,
                    dis->disable.location()
                );
            }
        }
    }

    void handle(const slang::syntax::PropertyDeclarationSyntax& decl) {
        if (decl.propertySpec) {
            checkPropertySpec(decl.propertySpec, decl.name.location());
        }
        visitDefault(decl);
    }

    void handle(const slang::syntax::ConcurrentAssertionStatementSyntax& stmt) {
        if (stmt.propertySpec) {
            checkPropertySpec(stmt.propertySpec, stmt.keyword.location());
        }
        visitDefault(stmt);
    }

private:
    core::DiagnosticEngine& diags;
};

class DisableIffComplexExpression : public Rule {
public:
    std::string_view getId() const override { return "SV-SVA-002"; }
    std::string_view getName() const override { return "DisableIffComplexExpression"; }
    std::string_view getDescription() const override {
        return "Flags 'disable iff (...)' clauses containing complex boolean expressions or multi-variable logic instead of simple reset conditions.";
    }
    std::string_view getCategory() const override { return "SVA"; }
    core::Severity getDefaultSeverity() const override { return core::Severity::Warning; }

    engine::RuleTask check(engine::RuleContext& ctx) override {
        auto& diags = ctx.diags();
        DisableIffComplexVisitor visitor(diags);
        co_await ctx.visitSyntax(visitor);
    }
};

} // namespace uvmcheck::rules::sva

namespace uvmcheck::rules {
std::unique_ptr<Rule> createDisableIffComplexExpression() {
    return std::make_unique<sva::DisableIffComplexExpression>();
}
} // namespace uvmcheck::rules
