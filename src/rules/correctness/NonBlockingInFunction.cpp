#include "uvmcheck/rules/Rule.h"
#include "uvmcheck/rules/RuleRegistry.h"
#include "slang/syntax/SyntaxVisitor.h"
#include "slang/syntax/AllSyntax.h"
#include "slang/syntax/SyntaxTree.h"
#include <algorithm>
#include <cctype>

namespace uvmcheck::rules::correctness {

class NonBlockingInFunctionVisitor : public slang::syntax::SyntaxVisitor<NonBlockingInFunctionVisitor> {
public:
    explicit NonBlockingInFunctionVisitor(core::DiagnosticEngine& diags) : diags(diags) {}

    void handle(const slang::syntax::FunctionDeclarationSyntax& func) {
        if (func.kind != slang::syntax::SyntaxKind::FunctionDeclaration) {
            visitDefault(func);
            return;
        }

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

    void handle(const slang::syntax::BinaryExpressionSyntax& expr) {
        if (inFunction && expr.kind == slang::syntax::SyntaxKind::NonblockingAssignmentExpression) {
            std::vector<core::FixItHint> fixIts;
            if (expr.operatorToken.range().start().valid()) {
                fixIts.push_back({
                    expr.operatorToken.range(),
                    "=",
                    "Replace non-blocking assignment '<=' with blocking assignment '='"
                });
            }
            diags.addViolation(
                "SV-LANG-005",
                "NonBlockingInFunction",
                "Non-blocking assignment '<=' used inside function '" + funcName +
                "'. In SystemVerilog (IEEE 1800 § 13.4.4), functions execute in zero simulation time and must only use blocking assignments ('='). Non-blocking assignments schedule updates into the NBA region, violating function combinational execution semantics.",
                core::Severity::Error,
                expr.operatorToken.location(),
                std::move(fixIts)
            );
        }
        visitDefault(expr);
    }

private:
    core::DiagnosticEngine& diags;
    bool inFunction = false;
    std::string funcName;
};

class NonBlockingInFunction : public Rule {
public:
    std::string_view getId() const override { return "SV-LANG-005"; }
    std::string_view getName() const override { return "NonBlockingInFunction"; }
    std::string_view getDescription() const override {
        return "Prohibits non-blocking assignments ('<=') inside functions. Functions execute in zero "
               "simulation time and must only use blocking assignments ('=') per IEEE 1800 § 13.4.4.";
    }
    std::string_view getCategory() const override { return "Correctness"; }
    core::Severity getDefaultSeverity() const override { return core::Severity::Error; }

    void run(slang::ast::Compilation& compilation,
             const uvmcheck::uvm::UvmModel& /*uvmModel*/,
             core::DiagnosticEngine& diags) override {
        NonBlockingInFunctionVisitor visitor(diags);
        for (const auto& syntaxTree : compilation.getSyntaxTrees()) {
            if (syntaxTree) {
                syntaxTree->root().visit(visitor);
            }
        }
    }
};

} // namespace uvmcheck::rules::correctness

namespace uvmcheck::rules {
std::unique_ptr<Rule> createNonBlockingInFunction() {
    return std::make_unique<correctness::NonBlockingInFunction>();
}
}
