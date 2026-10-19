#include "uvmcheck/rules/Rule.h"
#include "uvmcheck/rules/RuleRegistry.h"
#include "slang/syntax/AllSyntax.h"
#include "slang/syntax/SyntaxTree.h"
#include "slang/syntax/SyntaxVisitor.h"
#include <string>

namespace uvmcheck::rules::uvm {

class ConfigDbGetVisitor : public slang::syntax::SyntaxVisitor<ConfigDbGetVisitor> {
public:
    explicit ConfigDbGetVisitor(core::DiagnosticEngine& diags) : diags(diags) {}

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

        std::string leftStr = std::string(inv->left->toString());
        std::string methodName = std::string(inv->left->getLastToken().valueText());

        if (methodName == "get" && leftStr.find("uvm_config_db") != std::string::npos) {
            diags.addViolation(
                "UVM-CFG-004",
                "ConfigDbGetUnchecked",
                "Calling 'uvm_config_db::get()' without checking its return boolean status risks silent uninitialized state or null-pointer dereferences if the field is not found. Check the return value in an 'if' statement (e.g. 'if (!uvm_config_db#(...)::get(...)) `uvm_fatal(...)').",
                core::Severity::Warning,
                inv->getFirstToken().location()
            );
        }
    }
};

class ConfigDbGetUnchecked : public Rule {
public:
    std::string_view getId() const override { return "UVM-CFG-004"; }
    std::string_view getName() const override { return "ConfigDbGetUnchecked"; }
    std::string_view getDescription() const override {
        return "Flags calls to 'uvm_config_db#(...)::get()' where the boolean return status is ignored without being checked in an 'if' condition or assertion.";
    }
    std::string_view getCategory() const override { return "UVM"; }
    core::Severity getDefaultSeverity() const override { return core::Severity::Warning; }

    void run(slang::ast::Compilation& compilation,
             const uvmcheck::uvm::UvmModel& /*uvmModel*/,
             core::DiagnosticEngine& diags) override {
        ConfigDbGetVisitor visitor(diags);
        for (const auto& tree : compilation.getSyntaxTrees()) {
            if (tree) {
                tree->root().visit(visitor);
            }
        }
    }
};

} // namespace uvmcheck::rules::uvm

namespace uvmcheck::rules {
std::unique_ptr<Rule> createConfigDbGetUnchecked() {
    return std::make_unique<uvm::ConfigDbGetUnchecked>();
}
}
