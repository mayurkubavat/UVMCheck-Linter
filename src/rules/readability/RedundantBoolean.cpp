#include "uvmcheck/rules/Rule.h"
#include "uvmcheck/rules/RuleRegistry.h"
#include "uvmcheck/rules/SweSupport.h"
#include "slang/ast/expressions/LiteralExpressions.h"
#include "slang/ast/expressions/OperatorExpressions.h"
#include "slang/ast/statements/ConditionalStatements.h"
#include "slang/ast/statements/LoopStatements.h"
#include "slang/ast/statements/MiscStatements.h"

#include <unordered_set>

namespace uvmcheck::rules::readability {

using namespace slang::ast;

namespace {

// 0 or 1 for a 0/1 integer literal, -1 otherwise.
int boolLiteral(const Expression& e) {
    const Expression& x = swe::stripConversions(e);
    if (x.kind != ExpressionKind::IntegerLiteral) return -1;
    auto v = x.as<IntegerLiteral>().getValue().as<int64_t>();
    if (!v || (*v != 0 && *v != 1)) return -1;
    return static_cast<int>(*v);
}

bool isOneBit(const Expression& e) {
    const Expression& x = swe::stripConversions(e);
    return x.type && x.type->isIntegral() && x.type->getBitWidth() == 1;
}

const Statement* unwrap(const Statement& s) {
    const Statement* x = &s;
    while (x->kind == StatementKind::Block && x->as<BlockStatement>().blockKind == StatementBlockKind::Sequential)
        x = &x->as<BlockStatement>().body;
    if (x->kind == StatementKind::List && x->as<StatementList>().list.size() == 1) return unwrap(*x->as<StatementList>().list[0]);
    return x;
}

int returnedBool(const Statement& s) {
    const Statement* x = unwrap(s);
    if (x->kind != StatementKind::Return || !x->as<ReturnStatement>().expr) return -1;
    return boolLiteral(*x->as<ReturnStatement>().expr);
}

class RedundantVisitor : public swe::UserCodeVisitor<RedundantVisitor> {
public:
    using UserCodeVisitor::handle;

    explicit RedundantVisitor(core::DiagnosticEngine& diags) : diags(diags) {}

    void handle(const BinaryExpression& bin) {
        bool eq = bin.op == BinaryOperator::Equality || bin.op == BinaryOperator::Inequality;
        if (eq) {
            int l = boolLiteral(bin.left()), r = boolLiteral(bin.right());
            const Expression* other = l >= 0 ? &bin.right() : (r >= 0 ? &bin.left() : nullptr);
            if (other && boolLiteral(*other) < 0 && isOneBit(*other)) {
                report(swe::locOf(bin), "'" + swe::exprText(bin) + "' compares a 1-bit value with a constant; "
                       "write '" + std::string(((l >= 0 ? l : r) == 1) == (bin.op == BinaryOperator::Equality) ? "" : "!") +
                       swe::exprText(swe::stripConversions(*other)) + "' instead.");
            }
        }
        visitDefault(bin);
    }

    void handle(const ConditionalStatement& c) {
        // The last link of an else-if chain is not a stand-alone if/else.
        if (c.ifFalse && c.ifFalse->kind == StatementKind::Conditional) elseIfs.insert(&c.ifFalse->as<ConditionalStatement>());
        if (c.ifFalse && !elseIfs.count(&c) && c.conditions.size() == 1 && !c.conditions[0].pattern) {
            int t = returnedBool(c.ifTrue), f = returnedBool(*c.ifFalse);
            if (t >= 0 && f >= 0 && t != f) {
                report(swe::locOf(c), "'if (c) return " + std::to_string(t) + "; else return " +
                       std::to_string(f) + ";' can be written 'return " + (t ? "" : "!") + "(" +
                       swe::exprText(*c.conditions[0].expr) + ");'.");
            }
        }
        visitDefault(c);
    }

private:
    core::DiagnosticEngine& diags;
    std::unordered_set<const ConditionalStatement*> elseIfs;

    void report(slang::SourceLocation loc, const std::string& msg) {
        diags.addViolation("SV-RDB-005", "RedundantBoolean", msg, core::Severity::Info, loc);
    }
};

} // namespace

class RedundantBoolean : public Rule {
public:
    std::string_view getId() const override { return "SV-RDB-005"; }
    std::string_view getName() const override { return "RedundantBoolean"; }
    std::string_view getDescription() const override {
        return "Flags 1-bit values compared with 1'b1/1'b0 and 'if (c) return 1; else return 0;' "
               "(Sonar S1125, clang-tidy readability-simplify-boolean-expr). Advisory: off by default.";
    }
    std::string_view getCategory() const override { return "Advisory"; }
    core::Severity getDefaultSeverity() const override { return core::Severity::Info; }

    void run(Compilation& compilation, const uvm::UvmModel&, core::DiagnosticEngine& diags) override {
        RedundantVisitor v(diags);
        compilation.getRoot().visit(v);
    }
};

} // namespace uvmcheck::rules::readability

namespace uvmcheck::rules {
std::unique_ptr<Rule> createRedundantBoolean() {
    return std::make_unique<readability::RedundantBoolean>();
}
}
