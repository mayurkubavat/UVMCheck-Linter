#include "uvmcheck/rules/Rule.h"
#include "uvmcheck/rules/RuleRegistry.h"
#include "slang/syntax/AllSyntax.h"
#include "slang/syntax/SyntaxTree.h"
#include "slang/syntax/SyntaxVisitor.h"
#include <string>

namespace uvmcheck::rules::performance {

class InPlaceConcatFinder : public slang::syntax::SyntaxVisitor<InPlaceConcatFinder> {
public:
    explicit InPlaceConcatFinder(core::DiagnosticEngine& diags) : diags(diags) {}

    void handle(const slang::syntax::BinaryExpressionSyntax& expr) {
        if (expr.kind == slang::syntax::SyntaxKind::AssignmentExpression && expr.left && expr.right) {
            std::string varName = std::string(expr.left->getLastToken().valueText());
            if (!varName.empty()) {
                if (expr.right->kind == slang::syntax::SyntaxKind::ConcatenationExpression) {
                    std::string rightStr = std::string(expr.right->toString());
                    if (rightStr.find(varName) != std::string::npos) {
                        diags.addViolation(
                            "SV-PRF-010",
                            "StringConcatInLoop",
                            "Iterative concatenation of variable '" + varName +
                            "' inside a loop construct. In SystemVerilog, repeatedly concatenating strings or arrays inside loops causes quadratic heap allocations and memory copies (O(N^2)). Accumulate elements into a queue ('q.push_back(...)') or pre-allocate fixed buffers instead.",
                            core::Severity::Warning,
                            expr.operatorToken.location()
                        );
                    }
                } else if (expr.right->kind == slang::syntax::SyntaxKind::InvocationExpression) {
                    const auto& inv = expr.right->as<slang::syntax::InvocationExpressionSyntax>();
                    if (inv.left && inv.left->getLastToken().valueText() == "$sformatf") {
                        std::string rightStr = std::string(inv.toString());
                        if (rightStr.find(varName) != std::string::npos) {
                            diags.addViolation(
                                "SV-PRF-010",
                                "StringConcatInLoop",
                                "Iterative string formatting into '" + varName +
                                "' via $sformatf inside a loop construct. Repeatedly reallocating and copying strings inside loops causes severe memory fragmentation and simulation slowdown. Accumulate messages into a queue or use stream buffers instead.",
                                core::Severity::Warning,
                                expr.operatorToken.location()
                            );
                        }
                    }
                }
            }
        }
        visitDefault(expr);
    }

private:
    core::DiagnosticEngine& diags;
};

class LoopConcatVisitor : public slang::syntax::SyntaxVisitor<LoopConcatVisitor> {
public:
    explicit LoopConcatVisitor(core::DiagnosticEngine& diags) : diags(diags) {}

    void handle(const slang::syntax::ForLoopStatementSyntax& stmt) {
        InPlaceConcatFinder finder(diags);
        stmt.statement->visit(finder);
        visitDefault(stmt);
    }

    void handle(const slang::syntax::ForeachLoopStatementSyntax& stmt) {
        InPlaceConcatFinder finder(diags);
        stmt.statement->visit(finder);
        visitDefault(stmt);
    }

    void handle(const slang::syntax::LoopStatementSyntax& stmt) {
        InPlaceConcatFinder finder(diags);
        stmt.statement->visit(finder);
        visitDefault(stmt);
    }

    void handle(const slang::syntax::DoWhileStatementSyntax& stmt) {
        InPlaceConcatFinder finder(diags);
        stmt.statement->visit(finder);
        visitDefault(stmt);
    }

private:
    core::DiagnosticEngine& diags;
};

class StringConcatInLoop : public Rule {
public:
    std::string_view getId() const override { return "SV-PRF-010"; }
    std::string_view getName() const override { return "StringConcatInLoop"; }
    std::string_view getDescription() const override {
        return "Disallows iterative in-place string concatenation or $sformatf recursion inside loop bodies to prevent O(N^2) memory reallocation overhead.";
    }
    std::string_view getCategory() const override { return "Performance"; }
    core::Severity getDefaultSeverity() const override { return core::Severity::Warning; }

    void run(slang::ast::Compilation& compilation,
             const uvmcheck::uvm::UvmModel& /*uvmModel*/,
             core::DiagnosticEngine& diags) override {
        LoopConcatVisitor visitor(diags);
        for (const auto& tree : compilation.getSyntaxTrees()) {
            if (tree) {
                tree->root().visit(visitor);
            }
        }
    }
};

} // namespace uvmcheck::rules::performance

namespace uvmcheck::rules {
std::unique_ptr<Rule> createStringConcatInLoop() {
    return std::make_unique<performance::StringConcatInLoop>();
}
} // namespace uvmcheck::rules
