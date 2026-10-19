#include "uvmcheck/rules/Rule.h"
#include "uvmcheck/rules/RuleRegistry.h"
#include "slang/syntax/AllSyntax.h"
#include "slang/syntax/SyntaxTree.h"
#include "slang/syntax/SyntaxVisitor.h"

namespace uvmcheck::rules::concurrency {

class DisableForkVisitor : public slang::syntax::SyntaxVisitor<DisableForkVisitor> {
public:
    explicit DisableForkVisitor(core::DiagnosticEngine& diags) : diags(diags) {}

    void handle(const slang::syntax::DisableForkStatementSyntax& node) {
        // Trace ancestor hierarchy to verify if disable fork is enclosed in an isolating fork..join block
        bool hasEnclosingFork = false;
        const slang::syntax::SyntaxNode* curr = node.parent;

        while (curr) {
            // If we reach a routine or module boundary, stop
            if (curr->kind == slang::syntax::SyntaxKind::FunctionDeclaration ||
                curr->kind == slang::syntax::SyntaxKind::TaskDeclaration ||
                curr->kind == slang::syntax::SyntaxKind::ModuleDeclaration ||
                curr->kind == slang::syntax::SyntaxKind::ClassDeclaration) {
                break;
            }

            // An enclosing ParallelBlockStatement (fork .. join) provides thread isolation
            if (curr->kind == slang::syntax::SyntaxKind::ParallelBlockStatement) {
                hasEnclosingFork = true;
                break;
            }

            curr = curr->parent;
        }

        if (!hasEnclosingFork) {
            diags.addViolation(
                "SV-CON-010",
                "DisableForkWithoutScope",
                "Unenclosed 'disable fork;' executed directly at routine scope terminates all active child processes of the calling thread. "
                "In IEEE 1800 § 9.6.3 concurrency guidelines, 'disable fork;' must be enclosed inside an isolating "
                "'fork begin ... disable fork; end join' block to avoid inadvertently killing sibling or parent threads.",
                core::Severity::Error,
                node.disable.location()
            );
        }

        visitDefault(node);
    }

private:
    core::DiagnosticEngine& diags;
};

class DisableForkWithoutScope : public Rule {
public:
    std::string_view getId() const override { return "SV-CON-010"; }
    std::string_view getName() const override { return "DisableForkWithoutScope"; }
    std::string_view getDescription() const override {
        return "Disallows calling 'disable fork;' directly at task or routine scope without an enclosing isolating 'fork ... join' block.";
    }
    std::string_view getCategory() const override { return "Concurrency"; }
    core::Severity getDefaultSeverity() const override { return core::Severity::Error; }

    void run(slang::ast::Compilation& compilation,
             const uvmcheck::uvm::UvmModel& /*uvmModel*/,
             core::DiagnosticEngine& diags) override {
        DisableForkVisitor visitor(diags);
        for (const auto& tree : compilation.getSyntaxTrees()) {
            if (tree) {
                tree->root().visit(visitor);
            }
        }
    }
};

} // namespace uvmcheck::rules::concurrency

namespace uvmcheck::rules {
std::unique_ptr<Rule> createDisableForkWithoutScope() {
    return std::make_unique<concurrency::DisableForkWithoutScope>();
}
} // namespace uvmcheck::rules
