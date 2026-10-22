#include "uvmcheck/rules/Rule.h"
#include "uvmcheck/rules/RuleRegistry.h"
#include "slang/syntax/SyntaxVisitor.h"
#include "slang/syntax/AllSyntax.h"
#include "slang/syntax/SyntaxTree.h"

namespace uvmcheck::rules::sva {

class DisableIffResetVisitor : public engine::SyntaxVisitor<DisableIffResetVisitor> {
public:
    explicit DisableIffResetVisitor(core::DiagnosticEngine& diags) : diags(diags) {}

    void handle(const slang::syntax::PropertyDeclarationSyntax& decl) {
        if (decl.propertySpec && !decl.propertySpec->disable) {
            std::string propName = std::string(decl.name.valueText());
            diags.addViolation(
                "SVA-RST-001",
                "DisableIffReset",
                "Property declaration '" + propName + "' lacks a 'disable iff (!rst_n)' reset gating condition. "
                "Concurrent assertions without reset gating evaluate during power-on and warm reset cycles, "
                "causing spurious false-failure storms in simulation regressions.",
                core::Severity::Warning,
                decl.name.location()
            );
        }
        visitDefault(decl);
    }

    void handle(const slang::syntax::ConcurrentAssertionStatementSyntax& stmt) {
        if (stmt.keyword.kind == slang::parsing::TokenKind::AssertKeyword ||
            stmt.keyword.kind == slang::parsing::TokenKind::AssumeKeyword) {
            
            if (stmt.propertySpec && !stmt.propertySpec->disable) {
                // If it's not a reference to an external named property, check inline assertion
                const auto* innerExpr = stmt.propertySpec->expr.get();
                if (innerExpr &&
                    innerExpr->kind != slang::syntax::SyntaxKind::IdentifierName &&
                    innerExpr->kind != slang::syntax::SyntaxKind::InvocationExpression) {
                    
                    std::string kwStr = std::string(stmt.keyword.valueText());
                    diags.addViolation(
                        "SVA-RST-001",
                        "DisableIffReset",
                        "Inline concurrent assertion '" + kwStr + " property' lacks a 'disable iff (!rst_n)' reset gating condition. "
                        "Add a disable iff condition to prevent spurious assertion violations while the DUT is under reset.",
                        core::Severity::Warning,
                        stmt.keyword.location()
                    );
                }
            }
        }
        visitDefault(stmt);
    }

private:
    core::DiagnosticEngine& diags;
};

class DisableIffReset : public Rule {
public:
    std::string_view getId() const override { return "SVA-RST-001"; }
    std::string_view getName() const override { return "DisableIffReset"; }
    std::string_view getDescription() const override {
        return "Concurrent assertions and property declarations must include a 'disable iff (!rst_n)' clause "
               "to gate checking during power-on reset and warm resets, avoiding false-failure storms.";
    }
    std::string_view getCategory() const override { return "SVA"; }
    core::Severity getDefaultSeverity() const override { return core::Severity::Warning; }

    engine::RuleTask check(engine::RuleContext& ctx) override {
        auto& diags = ctx.diags();
        DisableIffResetVisitor visitor(diags);
        co_await ctx.visitSyntax(visitor);
    }
};

} // namespace uvmcheck::rules::sva

namespace uvmcheck::rules {
std::unique_ptr<Rule> createDisableIffReset() {
    return std::make_unique<sva::DisableIffReset>();
}
}
