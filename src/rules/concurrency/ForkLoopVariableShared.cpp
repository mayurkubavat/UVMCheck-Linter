#include "uvmcheck/rules/Rule.h"
#include "uvmcheck/rules/RuleRegistry.h"
#include "slang/ast/ASTVisitor.h"
#include "slang/ast/expressions/AssignmentExpressions.h"
#include "slang/ast/expressions/MiscExpressions.h"
#include "slang/ast/statements/LoopStatements.h"
#include "slang/ast/statements/MiscStatements.h"
#include "slang/ast/symbols/BlockSymbols.h"
#include <unordered_set>

namespace uvmcheck::rules::concurrency {

class ForkExprVisitor : public slang::ast::ASTVisitor<ForkExprVisitor, slang::ast::VisitFlags::Expressions> {
public:
    explicit ForkExprVisitor(const std::unordered_set<const slang::ast::Symbol*>& vars,
                             core::DiagnosticEngine& diags)
        : loopVars(vars), diags(diags) {}

    void handle(const slang::ast::NamedValueExpression& expr) {
        if (loopVars.count(&expr.symbol) > 0) {
            diags.addViolation(
                "SV-THR-001",
                "ForkLoopVariableShared",
                "Loop variable '" + std::string(expr.symbol.name) +
                "' is captured by reference inside a fork..join_none/any block. "
                "Copy to an automatic local variable before the fork to prevent race conditions.",
                core::Severity::Error,
                expr.sourceRange.start()
            );
        }
        visitDefault(expr);
    }

private:
    const std::unordered_set<const slang::ast::Symbol*>& loopVars;
    core::DiagnosticEngine& diags;
};

class LoopBodyVisitor : public slang::ast::ASTVisitor<LoopBodyVisitor, slang::ast::VisitFlags::Statements> {
public:
    explicit LoopBodyVisitor(const std::unordered_set<const slang::ast::Symbol*>& vars,
                             core::DiagnosticEngine& diags)
        : loopVars(vars), diags(diags) {}

    void handle(const slang::ast::BlockStatement& stmt) {
        if (stmt.blockKind == slang::ast::StatementBlockKind::JoinNone ||
            stmt.blockKind == slang::ast::StatementBlockKind::JoinAny) {
            ForkExprVisitor exprVisitor(loopVars, diags);
            stmt.body.visit(exprVisitor);
        }
        visitDefault(stmt);
    }

private:
    const std::unordered_set<const slang::ast::Symbol*>& loopVars;
    core::DiagnosticEngine& diags;
};

class ForLoopVisitor : public engine::AstVisitor<ForLoopVisitor> {
public:
    explicit ForLoopVisitor(core::DiagnosticEngine& diags) : diags(diags) {}

    void handle(const slang::ast::ForLoopStatement& loop) {
        std::unordered_set<const slang::ast::Symbol*> loopVars;

        for (const auto* var : loop.loopVars) {
            if (var) loopVars.insert(var);
        }

        for (const auto* init : loop.initializers) {
            if (init && init->kind == slang::ast::ExpressionKind::Assignment) {
                const auto& assign = init->as<slang::ast::AssignmentExpression>();
                if (assign.left().kind == slang::ast::ExpressionKind::NamedValue) {
                    loopVars.insert(&assign.left().as<slang::ast::NamedValueExpression>().symbol);
                }
            }
        }

        if (!loopVars.empty()) {
            LoopBodyVisitor bodyVisitor(loopVars, diags);
            loop.body.visit(bodyVisitor);
        }

        visitDefault(loop);
    }

private:
    core::DiagnosticEngine& diags;
};

class ForkLoopVariableShared : public Rule {
public:
    std::string_view getId() const override { return "SV-THR-001"; }
    std::string_view getName() const override { return "ForkLoopVariableShared"; }
    std::string_view getDescription() const override {
        return "Spawning background threads with fork..join_none inside loops without copying the "
               "loop variable to an automatic local variable causes all threads to see the final loop index.";
    }
    std::string_view getCategory() const override { return "Concurrency"; }
    core::Severity getDefaultSeverity() const override { return core::Severity::Error; }

    engine::RuleTask check(engine::RuleContext& ctx) override {
        auto& diags = ctx.diags();
        ForLoopVisitor visitor(diags);
        co_await ctx.visitAst(visitor);
    }
};

} // namespace uvmcheck::rules::concurrency

namespace uvmcheck::rules {
std::unique_ptr<Rule> createForkLoopVariableShared() {
    return std::make_unique<concurrency::ForkLoopVariableShared>();
}
}
