#include "uvmcheck/rules/Rule.h"
#include "uvmcheck/rules/RuleRegistry.h"
#include "slang/syntax/AllSyntax.h"
#include "slang/syntax/SyntaxTree.h"
#include "slang/syntax/SyntaxVisitor.h"
#include <string>

namespace uvmcheck::rules::deadcode {

class EmptyProceduralBlockVisitor : public slang::syntax::SyntaxVisitor<EmptyProceduralBlockVisitor> {
public:
    explicit EmptyProceduralBlockVisitor(core::DiagnosticEngine& diags) : diags(diags) {}

    void handle(const slang::syntax::ProceduralBlockSyntax& block) {
        if (isEffectivelyEmpty(block.statement)) {
            std::string kw = std::string(block.keyword.valueText());
            diags.addViolation(
                "SV-DED-009",
                "EmptyAlwaysOrInitial",
                "Procedural '" + kw + "' block contains an empty body. Empty procedural blocks consume simulator scheduling cycles with zero functional impact and frequently indicate unfinished or abandoned logic.",
                core::Severity::Warning,
                block.keyword.location()
            );
        }

        visitDefault(block);
    }

private:
    static bool isEffectivelyEmpty(const slang::syntax::StatementSyntax* stmt) {
        if (!stmt) return true;
        if (stmt->kind == slang::syntax::SyntaxKind::EmptyStatement) return true;
        if (stmt->kind == slang::syntax::SyntaxKind::SequentialBlockStatement ||
            stmt->kind == slang::syntax::SyntaxKind::ParallelBlockStatement) {
            const auto& b = stmt->as<slang::syntax::BlockStatementSyntax>();
            return b.items.empty();
        }
        if (stmt->kind == slang::syntax::SyntaxKind::TimingControlStatement) {
            const auto& tc = stmt->as<slang::syntax::TimingControlStatementSyntax>();
            return isEffectivelyEmpty(tc.statement);
        }
        return false;
    }

    core::DiagnosticEngine& diags;
};

class EmptyAlwaysOrInitial : public Rule {
public:
    std::string_view getId() const override { return "SV-DED-009"; }
    std::string_view getName() const override { return "EmptyAlwaysOrInitial"; }
    std::string_view getDescription() const override {
        return "Disallows initial, always, always_comb, always_ff, always_latch, or final procedural blocks with empty bodies.";
    }
    std::string_view getCategory() const override { return "DeadCode"; }
    core::Severity getDefaultSeverity() const override { return core::Severity::Warning; }

    void run(slang::ast::Compilation& compilation,
             const uvmcheck::uvm::UvmModel& /*uvmModel*/,
             core::DiagnosticEngine& diags) override {
        EmptyProceduralBlockVisitor visitor(diags);
        for (const auto& tree : compilation.getSyntaxTrees()) {
            if (tree) {
                tree->root().visit(visitor);
            }
        }
    }
};

} // namespace uvmcheck::rules::deadcode

namespace uvmcheck::rules {
std::unique_ptr<Rule> createEmptyAlwaysOrInitial() {
    return std::make_unique<deadcode::EmptyAlwaysOrInitial>();
}
} // namespace uvmcheck::rules
