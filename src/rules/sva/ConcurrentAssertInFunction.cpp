#include "uvmcheck/rules/Rule.h"
#include "uvmcheck/rules/RuleRegistry.h"
#include "slang/syntax/SyntaxVisitor.h"
#include "slang/syntax/AllSyntax.h"
#include "slang/syntax/SyntaxTree.h"
#include <algorithm>
#include <cctype>

namespace uvmcheck::rules::sva {

class ConcurrentInFuncCstVisitor : public slang::syntax::SyntaxVisitor<ConcurrentInFuncCstVisitor> {
public:
    explicit ConcurrentInFuncCstVisitor(core::DiagnosticEngine& diags) : diags(diags) {}

    void handle(const slang::syntax::FunctionDeclarationSyntax& func) {
        bool prevInFunc = inFunction;
        std::string prevName = funcName;

        inFunction = true;
        if (func.prototype && func.prototype->name) {
            funcName = std::string(func.prototype->name->toString());
            funcName.erase(
                std::remove_if(funcName.begin(), funcName.end(),
                               [](unsigned char c) { return std::isspace(c); }),
                funcName.end()
            );
        } else {
            funcName = "function";
        }

        visitDefault(func);

        inFunction = prevInFunc;
        funcName = prevName;
    }

    void handle(const slang::syntax::ConcurrentAssertionStatementSyntax& stmt) {
        if (inFunction) {
            diags.addViolation(
                "SVA-CTX-001",
                "ConcurrentAssertInFunction",
                "Illegal concurrent assertion inside function '" + funcName +
                "'. Functions in SystemVerilog (IEEE 1800 § 13.4.4 & § 16.14.1) execute in zero simulation time and cannot participate in clock-domain sampled value checking. Move the assertion to a module, interface, checker, or task.",
                core::Severity::Error,
                stmt.sourceRange().start()
            );
        }
        visitDefault(stmt);
    }

private:
    core::DiagnosticEngine& diags;
    bool inFunction = false;
    std::string funcName;
};

class ConcurrentAssertInFunction : public Rule {
public:
    std::string_view getId() const override { return "SVA-CTX-001"; }
    std::string_view getName() const override { return "ConcurrentAssertInFunction"; }
    std::string_view getDescription() const override {
        return "Disallow concurrent assertions ('assert/assume/cover property') inside functions. "
               "Functions execute in zero simulation time and lack clocking contexts required by SVA semantics.";
    }
    std::string_view getCategory() const override { return "SVA"; }
    core::Severity getDefaultSeverity() const override { return core::Severity::Error; }

    void run(slang::ast::Compilation& compilation,
             const uvmcheck::uvm::UvmModel& /*uvmModel*/,
             core::DiagnosticEngine& diags) override {
        ConcurrentInFuncCstVisitor visitor(diags);
        for (const auto& syntaxTree : compilation.getSyntaxTrees()) {
            if (syntaxTree) {
                syntaxTree->root().visit(visitor);
            }
        }
    }
};

} // namespace uvmcheck::rules::sva

namespace uvmcheck::rules {
std::unique_ptr<Rule> createConcurrentAssertInFunction() {
    return std::make_unique<sva::ConcurrentAssertInFunction>();
}
}
