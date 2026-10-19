#include "uvmcheck/rules/Rule.h"
#include "uvmcheck/rules/RuleRegistry.h"
#include "slang/syntax/AllSyntax.h"
#include "slang/syntax/SyntaxTree.h"
#include "slang/syntax/SyntaxVisitor.h"
#include <string>

namespace uvmcheck::rules::concurrency {

class ConstantWaitVisitor : public slang::syntax::SyntaxVisitor<ConstantWaitVisitor> {
public:
    explicit ConstantWaitVisitor(core::DiagnosticEngine& diags) : diags(diags) {}

    void handle(const slang::syntax::WaitStatementSyntax& stmt) {
        if (stmt.expr) {
            std::string exprStr = std::string(stmt.expr->toString());
            // Trim whitespace
            exprStr.erase(0, exprStr.find_first_not_of(" \t\n\r"));
            if (size_t end = exprStr.find_last_not_of(" \t\n\r"); end != std::string::npos) {
                exprStr.erase(end + 1);
            }

            bool isConstant = false;
            if (exprStr == "0" || exprStr == "1" ||
                exprStr == "1'b0" || exprStr == "1'b1" ||
                exprStr == "1'd0" || exprStr == "1'd1" ||
                exprStr == "32'd0" || exprStr == "32'd1" ||
                exprStr == "true" || exprStr == "false") {
                isConstant = true;
            } else if (stmt.expr->kind == slang::syntax::SyntaxKind::IntegerLiteralExpression ||
                       stmt.expr->kind == slang::syntax::SyntaxKind::IntegerVectorExpression) {
                isConstant = true;
            }

            if (isConstant) {
                diags.addViolation(
                    "SV-CON-004",
                    "ConstantWaitCondition",
                    "Procedural 'wait(" + exprStr + ")' specifies a compile-time constant condition. "
                    "'wait(0)' deadlocks the execution thread permanently; 'wait(1)' evaluates instantaneously without synchronization. "
                    "Use a dynamic signal expression or explicit event control ('@(event)').",
                    core::Severity::Error,
                    stmt.expr->getFirstToken().location()
                );
            }
        }

        visitDefault(stmt);
    }

private:
    core::DiagnosticEngine& diags;
};

class ConstantWaitCondition : public Rule {
public:
    std::string_view getId() const override { return "SV-CON-004"; }
    std::string_view getName() const override { return "ConstantWaitCondition"; }
    std::string_view getDescription() const override {
        return "Flags 'wait()' statements with compile-time constant conditions (e.g., 'wait(0);', 'wait(1);'). "
               "'wait(0);' deadlocks the simulation thread permanently, while 'wait(1);' executes instantaneously.";
    }
    std::string_view getCategory() const override { return "Concurrency"; }
    core::Severity getDefaultSeverity() const override { return core::Severity::Error; }

    void run(slang::ast::Compilation& compilation,
             const uvmcheck::uvm::UvmModel& /*uvmModel*/,
             core::DiagnosticEngine& diags) override {
        ConstantWaitVisitor visitor(diags);
        for (const auto& tree : compilation.getSyntaxTrees()) {
            if (tree) {
                tree->root().visit(visitor);
            }
        }
    }
};

} // namespace uvmcheck::rules::concurrency

namespace uvmcheck::rules {
std::unique_ptr<Rule> createConstantWaitCondition() {
    return std::make_unique<concurrency::ConstantWaitCondition>();
}
}
