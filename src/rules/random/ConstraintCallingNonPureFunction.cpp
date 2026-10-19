#include "uvmcheck/rules/Rule.h"
#include "uvmcheck/rules/RuleRegistry.h"
#include "slang/ast/ASTVisitor.h"
#include "slang/ast/symbols/SubroutineSymbols.h"
#include "slang/ast/expressions/CallExpression.h"
#include "slang/syntax/AllSyntax.h"
#include "slang/syntax/SyntaxTree.h"
#include "slang/syntax/SyntaxVisitor.h"
#include <string>

namespace uvmcheck::rules::random {

class ConstraintPureVisitor : public slang::syntax::SyntaxVisitor<ConstraintPureVisitor> {
public:
    explicit ConstraintPureVisitor(core::DiagnosticEngine& diags) : diags(diags) {}

    void handle(const slang::syntax::ConstraintDeclarationSyntax& cdecl) {
        insideConstraint = true;
        visitDefault(cdecl);
        insideConstraint = false;
    }

    void handle(const slang::syntax::ConstraintBlockSyntax& cblk) {
        insideConstraint = true;
        visitDefault(cblk);
        insideConstraint = false;
    }

    void handle(const slang::syntax::InvocationExpressionSyntax& inv) {
        if (insideConstraint && inv.left) {
            std::string funcName = std::string(inv.left->getLastToken().valueText());
            // Filter standard built-in functions that are allowed in constraints
            if (funcName != "inside" && funcName != "dist" && funcName != "size") {
                // Check if function name indicates a task or void function (e.g. print, display, task, void)
                bool isNonPure = (funcName.find("print") != std::string::npos ||
                                  funcName.find("display") != std::string::npos ||
                                  funcName.find("update") != std::string::npos ||
                                  funcName.find("set_") != std::string::npos ||
                                  funcName.find("void") != std::string::npos ||
                                  funcName.find("task") != std::string::npos);

                if (isNonPure) {
                    diags.addViolation(
                        "SV-RND-022",
                        "ConstraintCallingNonPureFunction",
                        "Function '" + funcName + "' invoked inside constraint block appears non-pure or mutates state. "
                        "In SystemVerilog (IEEE 1800 § 18.5.12), functions called inside constraint "
                        "expressions must be pure: they cannot modify class state, call tasks, or have void return types. "
                        "Impure functions cause constraint solver instability and unpredictable solution distributions.",
                        core::Severity::Error,
                        inv.left->getFirstToken().location()
                    );
                }
            }
        }
        visitDefault(inv);
    }

private:
    core::DiagnosticEngine& diags;
    bool insideConstraint = false;
};

class ConstraintCallingNonPureFunction : public Rule {
public:
    std::string_view getId() const override { return "SV-RND-022"; }
    std::string_view getName() const override { return "ConstraintCallingNonPureFunction"; }
    std::string_view getDescription() const override {
        return "Functions called in constraints must be pure and must not have side effects or void return types.";
    }
    std::string_view getCategory() const override { return "Randomization"; }
    core::Severity getDefaultSeverity() const override { return core::Severity::Error; }

    void run(slang::ast::Compilation& compilation,
             const uvmcheck::uvm::UvmModel& /*uvmModel*/,
             core::DiagnosticEngine& diags) override {
        ConstraintPureVisitor visitor(diags);
        for (const auto& tree : compilation.getSyntaxTrees()) {
            if (tree) {
                tree->root().visit(visitor);
            }
        }
    }
};

} // namespace uvmcheck::rules::random

namespace uvmcheck::rules {
std::unique_ptr<Rule> createConstraintCallingNonPureFunction() {
    return std::make_unique<random::ConstraintCallingNonPureFunction>();
}
} // namespace uvmcheck::rules
