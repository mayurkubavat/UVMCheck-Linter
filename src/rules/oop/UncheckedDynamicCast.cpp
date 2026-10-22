#include "uvmcheck/rules/Rule.h"
#include "uvmcheck/rules/RuleRegistry.h"
#include "slang/ast/ASTVisitor.h"
#include "slang/ast/expressions/CallExpression.h"
#include "slang/ast/expressions/ConversionExpression.h"
#include "slang/ast/statements/MiscStatements.h"

namespace uvmcheck::rules::oop {

class UncheckedCastVisitor : public engine::AstVisitor<UncheckedCastVisitor> {
public:
    explicit UncheckedCastVisitor(core::DiagnosticEngine& diags)
        : diags(diags) {}

    void handle(const slang::ast::ExpressionStatement& stmt) {
        checkExpr(stmt.expr, stmt.sourceRange);
        visitDefault(stmt);
    }

private:
    core::DiagnosticEngine& diags;

    void checkExpr(const slang::ast::Expression& expr, slang::SourceRange range) {
        // Direct call: $cast(dest, src);
        if (expr.kind == slang::ast::ExpressionKind::Call) {
            const auto& call = expr.as<slang::ast::CallExpression>();
            if (call.getSubroutineName() == "$cast") {
                reportViolation(range.start());
            }
        }
        // Void cast: void'($cast(dest, src));
        else if (expr.kind == slang::ast::ExpressionKind::Conversion) {
            const auto& conv = expr.as<slang::ast::ConversionExpression>();
            if (conv.type->isVoid() && conv.operand().kind == slang::ast::ExpressionKind::Call) {
                const auto& call = conv.operand().as<slang::ast::CallExpression>();
                if (call.getSubroutineName() == "$cast") {
                    reportViolation(range.start());
                }
            }
        }
    }

    void reportViolation(slang::SourceLocation loc) {
        diags.addViolation(
            "SV-OOP-002",
            "UncheckedDynamicCast",
            "Return value of $cast() is ignored or cast to void. If dynamic cast fails, destination "
            "handle remains null, risking runtime null-pointer dereferences.",
            core::Severity::Warning,
            loc
        );
    }
};

class UncheckedDynamicCast : public Rule {
public:
    std::string_view getId() const override { return "SV-OOP-002"; }
    std::string_view getName() const override { return "UncheckedDynamicCast"; }
    std::string_view getDescription() const override {
        return "The return value of $cast(dest, src) must be checked (e.g., in an if statement or assert) "
               "to prevent silent null-pointer dereferences on cast failures.";
    }
    std::string_view getCategory() const override { return "OOP"; }
    core::Severity getDefaultSeverity() const override { return core::Severity::Warning; }

    engine::RuleTask check(engine::RuleContext& ctx) override {
        auto& diags = ctx.diags();
        UncheckedCastVisitor visitor(diags);
        co_await ctx.visitAst(visitor);
    }
};

} // namespace uvmcheck::rules::oop

namespace uvmcheck::rules {
std::unique_ptr<Rule> createUncheckedDynamicCast() {
    return std::make_unique<oop::UncheckedDynamicCast>();
}
}
