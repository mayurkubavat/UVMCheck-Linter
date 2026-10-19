#include "uvmcheck/rules/Rule.h"
#include "uvmcheck/rules/RuleRegistry.h"
#include "slang/syntax/AllSyntax.h"
#include "slang/syntax/SyntaxTree.h"
#include "slang/syntax/SyntaxVisitor.h"
#include <unordered_set>
#include <string>

namespace uvmcheck::rules::correctness {

class ModifyingLoopIndexVisitor : public slang::syntax::SyntaxVisitor<ModifyingLoopIndexVisitor> {
public:
    explicit ModifyingLoopIndexVisitor(core::DiagnosticEngine& diags) : diags(diags) {}

    void handle(const slang::syntax::ForLoopStatementSyntax& forStmt) {
        std::unordered_set<std::string> loopVars;

        for (const auto* init : forStmt.initializers) {
            if (!init) continue;
            if (init->kind == slang::syntax::SyntaxKind::ForVariableDeclaration) {
                const auto& varDecl = init->as<slang::syntax::ForVariableDeclarationSyntax>();
                if (!varDecl.declarator->name.valueText().empty()) {
                    loopVars.insert(std::string(varDecl.declarator->name.valueText()));
                }
            } else if (init->kind == slang::syntax::SyntaxKind::AssignmentExpression) {
                const auto& bin = init->as<slang::syntax::BinaryExpressionSyntax>();
                if (bin.left && bin.left->kind == slang::syntax::SyntaxKind::IdentifierName) {
                    loopVars.insert(std::string(bin.left->as<slang::syntax::IdentifierNameSyntax>().identifier.valueText()));
                }
            }
        }

        if (!loopVars.empty() && forStmt.statement) {
            checkBody(*forStmt.statement, loopVars);
        }

        visitDefault(forStmt);
    }

private:
    core::DiagnosticEngine& diags;

    void checkBody(const slang::syntax::SyntaxNode& node, const std::unordered_set<std::string>& loopVars) {
        if (node.kind == slang::syntax::SyntaxKind::PostincrementExpression ||
            node.kind == slang::syntax::SyntaxKind::PostdecrementExpression) {
            const auto& post = node.as<slang::syntax::PostfixUnaryExpressionSyntax>();
            checkVarMod(post.operand, node.sourceRange().start(), loopVars);
        } else if (node.kind == slang::syntax::SyntaxKind::UnaryPreincrementExpression ||
                   node.kind == slang::syntax::SyntaxKind::UnaryPredecrementExpression) {
            const auto& pre = node.as<slang::syntax::PrefixUnaryExpressionSyntax>();
            checkVarMod(pre.operand, node.sourceRange().start(), loopVars);
        } else if (node.kind == slang::syntax::SyntaxKind::AssignmentExpression ||
                   node.kind == slang::syntax::SyntaxKind::AddAssignmentExpression ||
                   node.kind == slang::syntax::SyntaxKind::SubtractAssignmentExpression) {
            const auto& bin = node.as<slang::syntax::BinaryExpressionSyntax>();
            checkVarMod(bin.left, bin.operatorToken.location(), loopVars);
        }

        // Nested for loops declare their own scope; do not recurse into nested ForLoopStatements
        if (node.kind != slang::syntax::SyntaxKind::ForLoopStatement) {
            for (uint32_t i = 0; i < node.getChildCount(); i++) {
                const auto* child = node.childNode(i);
                if (child) {
                    checkBody(*child, loopVars);
                }
            }
        }
    }

    void checkVarMod(const slang::syntax::ExpressionSyntax* expr, slang::SourceLocation loc,
                     const std::unordered_set<std::string>& loopVars) {
        if (!expr) return;
        if (expr->kind == slang::syntax::SyntaxKind::IdentifierName) {
            std::string varName = std::string(expr->as<slang::syntax::IdentifierNameSyntax>().identifier.valueText());
            if (loopVars.count(varName)) {
                diags.addViolation(
                    "SV-COR-011",
                    "ModifyingLoopIndexInsideFor",
                    "Loop control variable '" + varName + "' is procedurally modified inside the body of the 'for' loop. "
                    "Altering the loop index within the loop body conflicts with the loop iteration step logic, "
                    "easily causing infinite loops, skipped elements, or subtle indexing off-by-one errors. "
                    "Move index adjustments into the loop update clause or use a 'while' loop.",
                    core::Severity::Warning,
                    loc
                );
            }
        }
    }
};

class ModifyingLoopIndexInsideFor : public Rule {
public:
    std::string_view getId() const override { return "SV-COR-011"; }
    std::string_view getName() const override { return "ModifyingLoopIndexInsideFor"; }
    std::string_view getDescription() const override {
        return "Flags modifications to the loop control variable inside a 'for' loop body. "
               "Mutating the loop index inside the body causes infinite loops, skipped iterations, and indexing errors.";
    }
    std::string_view getCategory() const override { return "Correctness"; }
    core::Severity getDefaultSeverity() const override { return core::Severity::Warning; }

    void run(slang::ast::Compilation& compilation,
             const uvmcheck::uvm::UvmModel& /*uvmModel*/,
             core::DiagnosticEngine& diags) override {
        ModifyingLoopIndexVisitor visitor(diags);
        for (const auto& tree : compilation.getSyntaxTrees()) {
            if (tree) {
                tree->root().visit(visitor);
            }
        }
    }
};

} // namespace uvmcheck::rules::correctness

namespace uvmcheck::rules {
std::unique_ptr<Rule> createModifyingLoopIndexInsideFor() {
    return std::make_unique<correctness::ModifyingLoopIndexInsideFor>();
}
}
