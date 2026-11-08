#pragma once

#include <cstdint>
#include <type_traits>

#include "slang/ast/ASTVisitor.h"
#include "slang/syntax/SyntaxVisitor.h"

namespace uvmcheck::engine {

/// Bookkeeping shared by both visitor bases so the single-pass dispatcher can
/// tell whether a handler asked to descend into the node it was given.
///
/// While the dispatcher calls `handle(node)`, `dispatchedNode` points at that
/// node. A `visitDefault(node)` on exactly that node is recorded instead of
/// performed, because the dispatcher walks the children itself. Any other
/// visit the handler makes (sub-walks of a child, a nested visitor, ...) runs
/// exactly as it would in a standalone walk.
struct DispatchState {
    const void* dispatchedNode = nullptr;
    bool descendRequested = false;
};

/// Drop-in replacement for slang::syntax::SyntaxVisitor that can take part in
/// the shared syntax walk. Behaves identically when used standalone.
template<typename TDerived>
class SyntaxVisitor : public slang::syntax::SyntaxVisitor<TDerived>, public DispatchState {
    using Base = slang::syntax::SyntaxVisitor<TDerived>;

public:
    void visitDefault(const slang::syntax::SyntaxNode& node) {
        if (dispatchedNode == &node) {
            descendRequested = true;
            return;
        }
        Base::visitDefault(node);
    }
};

/// Drop-in replacement for slang::ast::ASTVisitor that can take part in the
/// shared AST walk. Only VisitFlags::AllGood visitors are dispatched; any other
/// flag set still works but runs as its own walk.
template<typename TDerived, slang::ast::VisitFlags Flags = slang::ast::VisitFlags::AllGood>
class AstVisitor : public slang::ast::ASTVisitor<TDerived, Flags>, public DispatchState {
    using Base = slang::ast::ASTVisitor<TDerived, Flags>;

public:
    static constexpr bool Dispatchable = Flags == slang::ast::VisitFlags::AllGood;

    template<typename T>
    void visitDefault(const T& node) {
        if (dispatchedNode == static_cast<const void*>(&node)) {
            descendRequested = true;
            return;
        }
        Base::visitDefault(node);
    }
};

/// The AST node hierarchies slang's ASTVisitor walks. Each has its own kind enum.
enum class AstFamily : uint8_t {
    Symbol,
    Statement,
    Expression,
    TimingControl,
    Constraint,
    AssertionExpr,
    BinsSelectExpr,
    Pattern,
    Count
};

template<typename T>
constexpr AstFamily astFamilyOf() {
    using namespace slang::ast;
    if constexpr (std::is_base_of_v<Symbol, T>) return AstFamily::Symbol;
    else if constexpr (std::is_base_of_v<Statement, T>) return AstFamily::Statement;
    else if constexpr (std::is_base_of_v<Expression, T>) return AstFamily::Expression;
    else if constexpr (std::is_base_of_v<TimingControl, T>) return AstFamily::TimingControl;
    else if constexpr (std::is_base_of_v<Constraint, T>) return AstFamily::Constraint;
    else if constexpr (std::is_base_of_v<AssertionExpr, T>) return AstFamily::AssertionExpr;
    else if constexpr (std::is_base_of_v<BinsSelectExpr, T>) return AstFamily::BinsSelectExpr;
    else if constexpr (std::is_base_of_v<Pattern, T>) return AstFamily::Pattern;
    else return AstFamily::Count;
}

/// Type-erased view of a rule's syntax visitor, as seen by the dispatcher.
class SyntaxSubscriber {
public:
    virtual ~SyntaxSubscriber() = default;
    /// True if the visitor has a handle() overload for this node's type.
    virtual bool handles(const slang::syntax::SyntaxNode& node) = 0;
    /// Calls handle(node); returns true if the handler asked to descend.
    virtual bool dispatch(const slang::syntax::SyntaxNode& node) = 0;
    virtual bool wantsTokens() const = 0;
    virtual void token(slang::parsing::Token token) = 0;
    /// Runs the visitor as a standalone walk (isolated mode).
    virtual void walkStandalone(const slang::syntax::SyntaxNode& root) = 0;
};

/// Type-erased view of a rule's AST visitor, as seen by the dispatcher.
class AstSubscriber {
public:
    virtual ~AstSubscriber() = default;
    virtual bool handles(AstFamily family, const void* node) = 0;
    virtual bool dispatch(AstFamily family, const void* node) = 0;
    virtual void walkStandalone(const slang::ast::Symbol& root) = 0;
};

namespace detail {

template<typename V>
struct SyntaxShim {
    V& visitor;
    bool probe;
    bool handled = false;
    bool descend = false;

    template<typename T>
    void visit(const T& node) {
        if constexpr (requires { visitor.handle(node); }) {
            handled = true;
            if (!probe) {
                visitor.dispatchedNode = &node;
                visitor.descendRequested = false;
                visitor.handle(node);
                descend = visitor.descendRequested;
                visitor.dispatchedNode = nullptr;
            }
        }
    }
};

template<typename V>
class SyntaxSubscriberImpl final : public SyntaxSubscriber {
public:
    explicit SyntaxSubscriberImpl(V& v) : visitor(v) {}

    bool handles(const slang::syntax::SyntaxNode& node) override {
        SyntaxShim<V> shim{visitor, true};
        node.visit(shim);
        return shim.handled;
    }

    bool dispatch(const slang::syntax::SyntaxNode& node) override {
        SyntaxShim<V> shim{visitor, false};
        node.visit(shim);
        return shim.descend;
    }

    bool wantsTokens() const override {
        return requires(V& v, slang::parsing::Token t) { v.visitToken(t); };
    }

    void token(slang::parsing::Token t) override {
        if constexpr (requires { visitor.visitToken(t); }) visitor.visitToken(t);
    }

    void walkStandalone(const slang::syntax::SyntaxNode& root) override { root.visit(visitor); }

private:
    V& visitor;
};

template<typename V>
struct AstShim {
    V& visitor;
    bool probe;
    bool handled = false;
    bool descend = false;

    template<typename T>
    void visit(const T& node) {
        if constexpr (requires { visitor.handle(node); }) {
            handled = true;
            if (!probe) {
                visitor.dispatchedNode = static_cast<const void*>(&node);
                visitor.descendRequested = false;
                visitor.handle(node);
                descend = visitor.descendRequested;
                visitor.dispatchedNode = nullptr;
            }
        }
    }
};

template<typename V>
class AstSubscriberImpl final : public AstSubscriber {
public:
    explicit AstSubscriberImpl(V& v) : visitor(v) {}

    bool handles(AstFamily family, const void* node) override { return run(family, node, true).handled; }
    bool dispatch(AstFamily family, const void* node) override { return run(family, node, false).descend; }
    void walkStandalone(const slang::ast::Symbol& root) override { root.visit(visitor); }

private:
    AstShim<V> run(AstFamily family, const void* node, bool probe) {
        using namespace slang::ast;
        AstShim<V> shim{visitor, probe};
        switch (family) {
            case AstFamily::Symbol: static_cast<const Symbol*>(node)->visit(shim); break;
            case AstFamily::Statement: static_cast<const Statement*>(node)->visit(shim); break;
            case AstFamily::Expression: static_cast<const Expression*>(node)->visit(shim); break;
            case AstFamily::TimingControl: static_cast<const TimingControl*>(node)->visit(shim); break;
            case AstFamily::Constraint: static_cast<const Constraint*>(node)->visit(shim); break;
            case AstFamily::AssertionExpr: static_cast<const AssertionExpr*>(node)->visit(shim); break;
            case AstFamily::BinsSelectExpr: static_cast<const BinsSelectExpr*>(node)->visit(shim); break;
            case AstFamily::Pattern: static_cast<const Pattern*>(node)->visit(shim); break;
            case AstFamily::Count: break;
        }
        return shim;
    }

    V& visitor;
};

} // namespace detail

} // namespace uvmcheck::engine
