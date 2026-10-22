#pragma once

#include <coroutine>
#include <memory>
#include <vector>

#include "slang/ast/Compilation.h"
#include "slang/syntax/SyntaxTree.h"
#include "uvmcheck/core/Diagnostic.h"
#include "uvmcheck/engine/Visitors.h"
#include "uvmcheck/uvm/UvmModel.h"

namespace uvmcheck::engine {

enum class ExecutionMode {
    /// Every rule walks the design itself, one rule at a time (the old engine).
    /// Kept as a reference for equivalence testing and debugging.
    Isolated,
    /// One shared walk per shard dispatches nodes to every interested rule;
    /// shards and standalone rules run in parallel.
    SinglePass
};

/// Everything a rule sees while it runs. Each rule gets its own context and
/// its own DiagnosticEngine, so rules never share mutable state.
class RuleContext {
public:
    RuleContext(slang::ast::Compilation& compilation, const uvm::UvmModel& model,
                const slang::SourceManager& sourceManager, ExecutionMode mode)
        : compilation_(compilation), model_(model), diags_(sourceManager), mode_(mode) {}

    slang::ast::Compilation& compilation() { return compilation_; }
    const uvm::UvmModel& uvmModel() const { return model_; }
    core::DiagnosticEngine& diags() { return diags_; }
    ExecutionMode mode() const { return mode_; }

    /// `co_await ctx.visitSyntax(v, ...)` visits every syntax tree with the
    /// given visitors, in tree order, exactly as
    /// `for (tree : getSyntaxTrees()) tree->root().visit(v)` would.
    template<typename... V>
    auto visitSyntax(V&... visitors) {
        std::vector<std::unique_ptr<SyntaxSubscriber>> subs;
        (subs.push_back(std::make_unique<detail::SyntaxSubscriberImpl<V>>(visitors)), ...);
        return WalkAwaiter{*this, std::move(subs), {}};
    }

    /// `co_await ctx.visitAst(v, ...)` visits the elaborated design with the
    /// given visitors, exactly as `compilation.getRoot().visit(v)` would.
    template<typename... V>
    auto visitAst(V&... visitors) {
        static_assert((V::Dispatchable && ...),
                      "visitAst() needs engine::AstVisitor<..., VisitFlags::AllGood>; "
                      "walk other visitors directly");
        std::vector<std::unique_ptr<AstSubscriber>> subs;
        (subs.push_back(std::make_unique<detail::AstSubscriberImpl<V>>(visitors)), ...);
        return WalkAwaiter{*this, {}, std::move(subs)};
    }

    /// Used by the default Rule::check(): yields once so that rules which do
    /// their own walking are scheduled as independent parallel jobs.
    auto standaloneJob() { return WalkAwaiter{*this, {}, {}, true}; }

    // Engine-side state.
    std::vector<std::unique_ptr<SyntaxSubscriber>> pendingSyntax;
    std::vector<std::unique_ptr<AstSubscriber>> pendingAst;
    bool pendingStandalone = false;

private:
    struct WalkAwaiter {
        RuleContext& ctx;
        std::vector<std::unique_ptr<SyntaxSubscriber>> syntax;
        std::vector<std::unique_ptr<AstSubscriber>> ast;
        bool standalone = false;

        bool await_ready() {
            if (ctx.mode_ == ExecutionMode::SinglePass) return false;
            // Isolated mode: walk right here, no suspension.
            for (auto& s : syntax) {
                for (auto& tree : ctx.compilation_.getSyntaxTrees()) {
                    if (tree) s->walkStandalone(tree->root());
                }
            }
            for (auto& s : ast) s->walkStandalone(ctx.compilation_.getRoot());
            return true;
        }
        void await_suspend(std::coroutine_handle<>) {
            ctx.pendingSyntax = std::move(syntax);
            ctx.pendingAst = std::move(ast);
            ctx.pendingStandalone = standalone;
        }
        void await_resume() noexcept {}
    };

    slang::ast::Compilation& compilation_;
    const uvm::UvmModel& model_;
    core::DiagnosticEngine diags_;
    ExecutionMode mode_;
};

} // namespace uvmcheck::engine
