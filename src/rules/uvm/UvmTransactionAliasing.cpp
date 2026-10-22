#include "uvmcheck/rules/Rule.h"
#include "uvmcheck/rules/RuleRegistry.h"
#include "slang/syntax/AllSyntax.h"
#include "slang/syntax/SyntaxTree.h"
#include "slang/syntax/SyntaxVisitor.h"
#include <unordered_set>
#include <string>

namespace uvmcheck::rules::uvm {

class TransactionAliasingVisitor : public engine::SyntaxVisitor<TransactionAliasingVisitor> {
public:
    explicit TransactionAliasingVisitor(core::DiagnosticEngine& diags) : diags(diags) {}

    void handle(const slang::syntax::FunctionDeclarationSyntax& func) {
        checkItemList(func.items);
        visitDefault(func);
    }

    void handle(const slang::syntax::BlockStatementSyntax& block) {
        checkItemList(block.items);
        visitDefault(block);
    }

private:
    core::DiagnosticEngine& diags;

    void checkItemList(const slang::syntax::SyntaxList<slang::syntax::SyntaxNode>& items) {
        std::unordered_set<std::string> writtenTransactions;

        for (const auto* item : items) {
            if (item->kind != slang::syntax::SyntaxKind::ExpressionStatement) {
                continue;
            }

            const auto& exprStmt = item->as<slang::syntax::ExpressionStatementSyntax>();

            // Check if transaction was written to an analysis port: <port>.write(<trName>)
            if (exprStmt.expr->kind == slang::syntax::SyntaxKind::InvocationExpression) {
                const auto& inv = exprStmt.expr->as<slang::syntax::InvocationExpressionSyntax>();
                std::string methodName = std::string(inv.left->getLastToken().valueText());
                if (methodName == "write" && inv.arguments && !inv.arguments->parameters.empty()) {
                    const auto* firstArg = inv.arguments->parameters[0];
                    if (firstArg->kind == slang::syntax::SyntaxKind::OrderedArgument) {
                        const auto& ordArg = firstArg->as<slang::syntax::OrderedArgumentSyntax>();
                        std::string trName = std::string(ordArg.expr->getLastToken().valueText());
                        if (!trName.empty() && trName != "this") {
                            writtenTransactions.insert(trName);
                        }
                    }
                }
            }

            // Check if transaction is re-allocated: trName = new(...)
            if (exprStmt.expr->kind == slang::syntax::SyntaxKind::AssignmentExpression) {
                const auto& assign = exprStmt.expr->as<slang::syntax::BinaryExpressionSyntax>();
                std::string lhs = std::string(assign.left->getLastToken().valueText());
                std::string rhsStr = std::string(assign.right->toString());
                if (rhsStr.find("new") != std::string::npos || rhsStr.find("clone") != std::string::npos) {
                    writtenTransactions.erase(lhs);
                }

                // Check if a field of a written transaction is mutated in-place: trName.<field> = ...
                std::string baseTr;
                if (assign.left->kind == slang::syntax::SyntaxKind::MemberAccessExpression) {
                    const auto& memberAccess = assign.left->as<slang::syntax::MemberAccessExpressionSyntax>();
                    baseTr = std::string(memberAccess.left->getLastToken().valueText());
                } else if (assign.left->kind == slang::syntax::SyntaxKind::ScopedName) {
                    const auto& scoped = assign.left->as<slang::syntax::ScopedNameSyntax>();
                    baseTr = std::string(scoped.left->getLastToken().valueText());
                }

                if (!baseTr.empty() && writtenTransactions.count(baseTr) > 0) {
                    diags.addViolation(
                        "UVM-ALS-001",
                        "UvmTransactionAliasing",
                        "Transaction handle '" + baseTr + "' was broadcast to an analysis port ('write(" + baseTr +
                        ")') and subsequently mutated in-place without cloning. In UVM, analysis ports pass handles by reference; "
                        "in-place mutations corrupt transactions already stored by downstream scoreboards and coverage collectors. "
                        "Clone the transaction before writing or allocate a new instance with 'new()'.",
                        core::Severity::Warning,
                        assign.left->getFirstToken().location()
                    );
                    writtenTransactions.erase(baseTr);
                }
            }
        }
    }
};

class UvmTransactionAliasing : public Rule {
public:
    std::string_view getId() const override { return "UVM-ALS-001"; }
    std::string_view getName() const override { return "UvmTransactionAliasing"; }
    std::string_view getDescription() const override {
        return "Disallows in-place mutation of transaction objects after broadcasting via an analysis port (.write()). "
               "Analysis ports pass shared references; subsequent in-place mutations corrupt downstream scoreboards and subscribers.";
    }
    std::string_view getCategory() const override { return "UVM"; }
    core::Severity getDefaultSeverity() const override { return core::Severity::Warning; }

    engine::RuleTask check(engine::RuleContext& ctx) override {
        auto& diags = ctx.diags();
        TransactionAliasingVisitor visitor(diags);
        co_await ctx.visitSyntax(visitor);
    }
};

} // namespace uvmcheck::rules::uvm

namespace uvmcheck::rules {
std::unique_ptr<Rule> createUvmTransactionAliasing() {
    return std::make_unique<uvm::UvmTransactionAliasing>();
}
}
