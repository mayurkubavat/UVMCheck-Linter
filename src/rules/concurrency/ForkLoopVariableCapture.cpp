#include "uvmcheck/rules/Rule.h"
#include "uvmcheck/rules/RuleRegistry.h"
#include "slang/syntax/AllSyntax.h"
#include "slang/syntax/SyntaxTree.h"
#include "slang/syntax/SyntaxVisitor.h"
#include <unordered_set>
#include <string>

namespace uvmcheck::rules::concurrency {

class IdentifierFinder : public slang::syntax::SyntaxVisitor<IdentifierFinder> {
public:
    std::unordered_set<std::string> ids;
    std::unordered_set<std::string> localDeclarations;

    void handle(const slang::syntax::DataDeclarationSyntax& decl) {
        for (const auto* declarator : decl.declarators) {
            if (declarator) {
                localDeclarations.insert(std::string(declarator->name.valueText()));
            }
        }
        visitDefault(decl);
    }

    void handle(const slang::syntax::IdentifierNameSyntax& id) {
        std::string name = std::string(id.identifier.valueText());
        if (localDeclarations.count(name) == 0) {
            ids.insert(name);
        }
        visitDefault(id);
    }
};

class ForkInLoopVisitor : public slang::syntax::SyntaxVisitor<ForkInLoopVisitor> {
public:
    explicit ForkInLoopVisitor(core::DiagnosticEngine& diags) : diags(diags) {}

    void inspectForkInLoop(const slang::syntax::BlockStatementSyntax& block,
                           const std::unordered_set<std::string>& loopVars) {
        if (block.kind != slang::syntax::SyntaxKind::ParallelBlockStatement) return;

        auto joinKind = block.end.kind;
        if (joinKind != slang::parsing::TokenKind::JoinNoneKeyword &&
            joinKind != slang::parsing::TokenKind::JoinAnyKeyword) {
            return;
        }

        IdentifierFinder idFinder;
        block.visit(idFinder);

        for (const auto& varName : loopVars) {
            if (idFinder.ids.count(varName) > 0) {
                std::string joinKw = std::string(block.end.valueText());
                diags.addViolation(
                    "SV-CON-012",
                    "ForkLoopVariableCapture",
                    "Loop variable '" + varName + "' is referenced directly inside a 'fork ... " +
                    joinKw + "' child thread without an isolating local variable copy. "
                    "In SystemVerilog (IEEE 1800 § 9.3.2), threads spawned "
                    "with join_none or join_any execute after the loop advances, causing all spawned child threads to read "
                    "the final mutated value of the loop variable. Declare an automatic local variable copy "
                    "(e.g. 'automatic int _" + varName + " = " + varName + ";') inside the fork block to bind the per-iteration value.",
                    core::Severity::Error,
                    block.getFirstToken().location()
                );
                break;
            }
        }
    }

    void handle(const slang::syntax::ForLoopStatementSyntax& loop) {
        std::unordered_set<std::string> loopVars;
        for (const auto* init : loop.initializers) {
            if (init) {
                if (init->kind == slang::syntax::SyntaxKind::ForVariableDeclaration) {
                    const auto& fvd = init->as<slang::syntax::ForVariableDeclarationSyntax>();
                    if (fvd.declarator) {
                        loopVars.insert(std::string(fvd.declarator->name.valueText()));
                    }
                } else if (init->kind == slang::syntax::SyntaxKind::AssignmentExpression) {
                    const auto& bin = init->as<slang::syntax::BinaryExpressionSyntax>();
                    if (bin.left) {
                        loopVars.insert(std::string(bin.left->getFirstToken().valueText()));
                    }
                }
            }
        }

        if (!loopVars.empty() && loop.statement) {
            struct InnerForkFinder : public slang::syntax::SyntaxVisitor<InnerForkFinder> {
                ForkInLoopVisitor& parent;
                const std::unordered_set<std::string>& vars;
                InnerForkFinder(ForkInLoopVisitor& parent, const std::unordered_set<std::string>& vars)
                    : parent(parent), vars(vars) {}

                void handle(const slang::syntax::BlockStatementSyntax& block) {
                    parent.inspectForkInLoop(block, vars);
                    visitDefault(block);
                }
            } innerFinder(*this, loopVars);

            loop.statement->visit(innerFinder);
        }
        visitDefault(loop);
    }

    void handle(const slang::syntax::ForeachLoopStatementSyntax& loop) {
        std::unordered_set<std::string> loopVars;
        if (loop.loopList) {
            for (const auto* name : loop.loopList->loopVariables) {
                if (name) {
                    loopVars.insert(std::string(name->getFirstToken().valueText()));
                }
            }
        }

        if (!loopVars.empty() && loop.statement) {
            struct InnerForkFinder : public slang::syntax::SyntaxVisitor<InnerForkFinder> {
                ForkInLoopVisitor& parent;
                const std::unordered_set<std::string>& vars;
                InnerForkFinder(ForkInLoopVisitor& parent, const std::unordered_set<std::string>& vars)
                    : parent(parent), vars(vars) {}

                void handle(const slang::syntax::BlockStatementSyntax& block) {
                    parent.inspectForkInLoop(block, vars);
                    visitDefault(block);
                }
            } innerFinder(*this, loopVars);

            loop.statement->visit(innerFinder);
        }
        visitDefault(loop);
    }

private:
    core::DiagnosticEngine& diags;
};

class ForkLoopVariableCapture : public Rule {
public:
    std::string_view getId() const override { return "SV-CON-012"; }
    std::string_view getName() const override { return "ForkLoopVariableCapture"; }
    std::string_view getDescription() const override {
        return "Disallows referencing loop variables directly inside 'fork ... join_none' or 'join_any' blocks without an automatic local variable copy.";
    }
    std::string_view getCategory() const override { return "Concurrency"; }
    core::Severity getDefaultSeverity() const override { return core::Severity::Error; }

    void run(slang::ast::Compilation& compilation,
             const uvmcheck::uvm::UvmModel& /*uvmModel*/,
             core::DiagnosticEngine& diags) override {
        ForkInLoopVisitor visitor(diags);
        for (const auto& tree : compilation.getSyntaxTrees()) {
            if (tree) {
                tree->root().visit(visitor);
            }
        }
    }
};

} // namespace uvmcheck::rules::concurrency

namespace uvmcheck::rules {
std::unique_ptr<Rule> createForkLoopVariableCapture() {
    return std::make_unique<concurrency::ForkLoopVariableCapture>();
}
} // namespace uvmcheck::rules
