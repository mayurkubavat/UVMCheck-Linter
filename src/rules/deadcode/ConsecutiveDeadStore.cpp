#include "uvmcheck/rules/Rule.h"
#include "uvmcheck/rules/RuleRegistry.h"
#include "slang/ast/ASTVisitor.h"
#include "slang/ast/Statement.h"
#include "slang/ast/expressions/AssignmentExpressions.h"
#include "slang/ast/expressions/MiscExpressions.h"
#include "slang/ast/statements/MiscStatements.h"

namespace uvmcheck::rules::deadcode {

class SymbolSearchVisitor : public slang::ast::ASTVisitor<SymbolSearchVisitor, slang::ast::VisitFlags::AllGood> {
public:
    explicit SymbolSearchVisitor(const slang::ast::Symbol& target) : target(target) {}

    void handle(const slang::ast::NamedValueExpression& expr) {
        if (&expr.symbol == &target) {
            found = true;
        }
        visitDefault(expr);
    }

    const slang::ast::Symbol& target;
    bool found = false;
};

static const slang::ast::Symbol* getTargetSymbol(const slang::ast::Statement* stmt, const slang::ast::Expression*& rhsOut) {
    if (!stmt || stmt->kind != slang::ast::StatementKind::ExpressionStatement) return nullptr;
    const auto& exprStmt = stmt->as<slang::ast::ExpressionStatement>();
    if (exprStmt.expr.kind != slang::ast::ExpressionKind::Assignment) return nullptr;
    const auto& assign = exprStmt.expr.as<slang::ast::AssignmentExpression>();
    if (assign.left().kind != slang::ast::ExpressionKind::NamedValue) return nullptr;
    rhsOut = &assign.right();
    return &assign.left().as<slang::ast::NamedValueExpression>().symbol;
}

class DeadStoreVisitor : public engine::AstVisitor<DeadStoreVisitor> {
public:
    explicit DeadStoreVisitor(core::DiagnosticEngine& diags) : diags(diags) {}

    void handle(const slang::ast::StatementList& stmtList) {
        if (stmtList.list.size() >= 2) {
            for (size_t i = 0; i < stmtList.list.size() - 1; ++i) {
                const slang::ast::Expression* rhs1 = nullptr;
                const auto* sym1 = getTargetSymbol(stmtList.list[i], rhs1);
                if (!sym1) continue;

                const slang::ast::Expression* rhs2 = nullptr;
                const auto* sym2 = getTargetSymbol(stmtList.list[i + 1], rhs2);
                if (!sym2 || sym1 != sym2) continue;

                // Check if rhs2 reads sym1
                SymbolSearchVisitor search(*sym1);
                rhs2->visit(search);

                if (!search.found) {
                    diags.addViolation(
                        "SV-DED-004",
                        "ConsecutiveDeadStore",
                        "Dead store to variable '" + std::string(sym1->name) + "'. "
                        "The assigned value is unconditionally overwritten by the next assignment statement before ever being read.",
                        core::Severity::Warning,
                        stmtList.list[i]->sourceRange.start()
                    );
                }
            }
        }
        visitDefault(stmtList);
    }

private:
    core::DiagnosticEngine& diags;
};

class ConsecutiveDeadStore : public Rule {
public:
    std::string_view getId() const override { return "SV-DED-004"; }
    std::string_view getName() const override { return "ConsecutiveDeadStore"; }
    std::string_view getDescription() const override {
        return "Detects consecutive assignments to the same variable where the initial value is immediately overwritten "
               "without being read. Dead stores waste CPU cycles and frequently reveal copy-paste logic bugs.";
    }
    std::string_view getCategory() const override { return "DeadCode"; }
    core::Severity getDefaultSeverity() const override { return core::Severity::Warning; }

    engine::RuleTask check(engine::RuleContext& ctx) override {
        auto& diags = ctx.diags();
        DeadStoreVisitor visitor(diags);
        co_await ctx.visitAst(visitor);
    }
};

} // namespace uvmcheck::rules::deadcode

namespace uvmcheck::rules {
std::unique_ptr<Rule> createConsecutiveDeadStore() {
    return std::make_unique<deadcode::ConsecutiveDeadStore>();
}
}
