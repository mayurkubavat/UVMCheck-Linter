#include "uvmcheck/rules/Rule.h"
#include "uvmcheck/rules/RuleRegistry.h"
#include "slang/syntax/AllSyntax.h"
#include "slang/syntax/SyntaxTree.h"
#include "slang/syntax/SyntaxVisitor.h"
#include <string>

namespace uvmcheck::rules::concurrency {

class UnboundedMailboxVisitor : public engine::SyntaxVisitor<UnboundedMailboxVisitor> {
public:
    explicit UnboundedMailboxVisitor(core::DiagnosticEngine& diags) : diags(diags) {}

    void handle(const slang::syntax::DataDeclarationSyntax& decl) {
        if (decl.type) {
            std::string typeStr = std::string(decl.type->getLastToken().valueText());
            if (typeStr == "mailbox" || decl.type->toString().find("mailbox") != std::string::npos) {
                for (const auto* d : decl.declarators) {
                    if (d && d->initializer && d->initializer->expr) {
                        checkNewExpr(d->initializer->expr, std::string(d->name.valueText()));
                    }
                }
            }
        }
        visitDefault(decl);
    }

    void handle(const slang::syntax::BinaryExpressionSyntax& expr) {
        if (expr.kind == slang::syntax::SyntaxKind::AssignmentExpression) {
            std::string leftStr = std::string(expr.left->getLastToken().valueText());
            if (leftStr.find("mailbox") != std::string::npos ||
                leftStr.ends_with("_mb") || leftStr.ends_with("_mbox") || leftStr.starts_with("mb_")) {
                checkNewExpr(expr.right, leftStr);
            }
        }
        visitDefault(expr);
    }

private:
    void checkNewExpr(const slang::syntax::ExpressionSyntax* expr, const std::string& varName) {
        if (!expr) return;
        if (expr->kind == slang::syntax::SyntaxKind::NewClassExpression) {
            const auto& newExpr = expr->as<slang::syntax::NewClassExpressionSyntax>();
            bool isUnbounded = false;
            if (newExpr.argList == nullptr || newExpr.argList->parameters.empty()) {
                isUnbounded = true;
            } else {
                const auto* firstArg = newExpr.argList->parameters[0];
                if (firstArg) {
                    std::string argStr = std::string(firstArg->toString());
                    argStr.erase(0, argStr.find_first_not_of(" \t\n\r"));
                    if (size_t end = argStr.find_last_not_of(" \t\n\r"); end != std::string::npos) {
                        argStr.erase(end + 1);
                    }
                    if (argStr == "0" || argStr == "32'd0") {
                        isUnbounded = true;
                    }
                }
            }

            if (isUnbounded) {
                diags.addViolation(
                    "SV-CON-007",
                    "UnboundedMailboxInstantiation",
                    "Mailbox '" + varName +
                    "' is instantiated with an unbounded queue limit (bound = 0 or omitted). Per IEEE 1800 § 15.4.1, unbounded mailboxes allow producers to allocate unlimited transaction memory without blocking, risking memory exhaustion and masking downstream testbench stalls. Specify a bounded capacity (e.g. 'new(16)').",
                    core::Severity::Warning,
                    newExpr.scopedNew->getFirstToken().location()
                );
            }
        }
    }

    core::DiagnosticEngine& diags;
};

class UnboundedMailboxInstantiation : public Rule {
public:
    std::string_view getId() const override { return "SV-CON-007"; }
    std::string_view getName() const override { return "UnboundedMailboxInstantiation"; }
    std::string_view getDescription() const override {
        return "Disallows instantiating unbounded mailboxes without a capacity limit to prevent memory bloat and missed producer-consumer synchronization.";
    }
    std::string_view getCategory() const override { return "Concurrency"; }
    core::Severity getDefaultSeverity() const override { return core::Severity::Warning; }

    engine::RuleTask check(engine::RuleContext& ctx) override {
        auto& diags = ctx.diags();
        UnboundedMailboxVisitor visitor(diags);
        co_await ctx.visitSyntax(visitor);
    }
};

} // namespace uvmcheck::rules::concurrency

namespace uvmcheck::rules {
std::unique_ptr<Rule> createUnboundedMailboxInstantiation() {
    return std::make_unique<concurrency::UnboundedMailboxInstantiation>();
}
} // namespace uvmcheck::rules
