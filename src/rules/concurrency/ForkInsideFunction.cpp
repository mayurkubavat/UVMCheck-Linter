#include "uvmcheck/rules/Rule.h"
#include "uvmcheck/rules/RuleRegistry.h"
#include "slang/syntax/AllSyntax.h"
#include "slang/syntax/SyntaxTree.h"
#include "slang/syntax/SyntaxVisitor.h"

namespace uvmcheck::rules::concurrency {

class ForkInsideFunctionVisitor : public slang::syntax::SyntaxVisitor<ForkInsideFunctionVisitor> {
public:
    explicit ForkInsideFunctionVisitor(core::DiagnosticEngine& diags) : diags(diags) {}

    void handle(const slang::syntax::FunctionDeclarationSyntax& decl) {
        if (decl.kind == slang::syntax::SyntaxKind::FunctionDeclaration) {
            std::string prevFunc = currentFunctionName;
            bool prevInside = insideFunction;

            currentFunctionName = std::string(decl.prototype->name->getLastToken().valueText());
            insideFunction = true;

            visitDefault(decl);

            insideFunction = prevInside;
            currentFunctionName = prevFunc;
        } else {
            visitDefault(decl);
        }
    }

    void handle(const slang::syntax::BlockStatementSyntax& stmt) {
        if (stmt.kind == slang::syntax::SyntaxKind::ParallelBlockStatement && insideFunction) {
            std::string joinKind = std::string(stmt.end.valueText());
            diags.addViolation(
                "SV-FOR-001",
                "ForkInsideFunction",
                "Spawning concurrent background process with 'fork ... " + joinKind +
                "' inside function '" + currentFunctionName + "()'. "
                "IEEE 1800 § 13.4.4 requires functions to remain non-time-consuming and instantaneous; "
                "spawning threads from functions creates untracked background execution and risks corrupting local automatic variables after the function returns. "
                "Refactor concurrent thread creation to a task.",
                core::Severity::Warning,
                stmt.begin.location()
            );
        }
        visitDefault(stmt);
    }

private:
    core::DiagnosticEngine& diags;
    bool insideFunction = false;
    std::string currentFunctionName;
};

class ForkInsideFunction : public Rule {
public:
    std::string_view getId() const override { return "SV-FOR-001"; }
    std::string_view getName() const override { return "ForkInsideFunction"; }
    std::string_view getDescription() const override {
        return "Disallows spawning concurrent threads with 'fork ... join/join_any/join_none' inside functions. "
               "Functions must remain synchronous and instantaneous per IEEE 1800 § 13.4.4.";
    }
    std::string_view getCategory() const override { return "Concurrency"; }
    core::Severity getDefaultSeverity() const override { return core::Severity::Warning; }

    void run(slang::ast::Compilation& compilation,
             const uvmcheck::uvm::UvmModel& /*uvmModel*/,
             core::DiagnosticEngine& diags) override {
        ForkInsideFunctionVisitor visitor(diags);
        for (const auto& tree : compilation.getSyntaxTrees()) {
            if (tree) {
                tree->root().visit(visitor);
            }
        }
    }
};

} // namespace uvmcheck::rules::concurrency

namespace uvmcheck::rules {
std::unique_ptr<Rule> createForkInsideFunction() {
    return std::make_unique<concurrency::ForkInsideFunction>();
}
}
