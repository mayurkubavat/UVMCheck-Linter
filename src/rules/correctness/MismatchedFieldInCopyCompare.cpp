#include "uvmcheck/rules/Rule.h"
#include "uvmcheck/rules/RuleRegistry.h"
#include "uvmcheck/rules/SweSupport.h"
#include "slang/ast/expressions/AssignmentExpressions.h"
#include "slang/ast/expressions/MiscExpressions.h"
#include "slang/ast/expressions/OperatorExpressions.h"
#include "slang/ast/expressions/SelectExpressions.h"
#include "slang/ast/symbols/ClassSymbols.h"

namespace uvmcheck::rules::correctness {

using namespace slang::ast;

namespace {

const Expression& stripSelects(const Expression& expr) {
    const Expression* e = &swe::stripConversions(expr);
    while (true) {
        if (e->kind == ExpressionKind::ElementSelect) e = &swe::stripConversions(e->as<ElementSelectExpression>().value());
        else if (e->kind == ExpressionKind::RangeSelect) e = &swe::stripConversions(e->as<RangeSelectExpression>().value());
        else return *e;
    }
}

bool isThis(const Expression& e) {
    return e.kind == ExpressionKind::NamedValue && e.as<NamedValueExpression>().symbol.name == "this";
}

// A property of the object running the method: `addr` or `this.addr`.
const Symbol* ownField(const Expression& expr) {
    const Expression& e = stripSelects(expr);
    if (e.kind == ExpressionKind::NamedValue) {
        const auto& sym = e.as<NamedValueExpression>().symbol;
        if (sym.kind == SymbolKind::ClassProperty) return &sym;
    } else if (e.kind == ExpressionKind::MemberAccess) {
        const auto& ma = e.as<MemberAccessExpression>();
        if (ma.member.kind == SymbolKind::ClassProperty && isThis(swe::stripConversions(ma.value())))
            return &ma.member;
    }
    return nullptr;
}

// A property of another handle: `rhs_.addr`. Returns the member and the handle's class.
std::pair<const Symbol*, const ClassType*> otherField(const Expression& expr) {
    const Expression& e = stripSelects(expr);
    if (e.kind != ExpressionKind::MemberAccess) return {nullptr, nullptr};
    const auto& ma = e.as<MemberAccessExpression>();
    const Expression& base = swe::stripConversions(ma.value());
    if (ma.member.kind != SymbolKind::ClassProperty || isThis(base)) return {nullptr, nullptr};
    if (base.kind != ExpressionKind::NamedValue) return {nullptr, nullptr};
    return {&ma.member, swe::asClass(*base.type)};
}

bool classHasField(const ClassType* cls, std::string_view name) {
    int guard = 0;
    while (cls && guard++ < 64) {
        if (const Symbol* s = cls->find(name); s && s->kind == SymbolKind::ClassProperty) return true;
        const Type* base = cls->getBaseClass();
        cls = base ? swe::asClass(*base) : nullptr;
    }
    return false;
}

bool isFieldwiseMethod(std::string_view name) {
    return name.starts_with("do_") || name.find("copy") != std::string_view::npos ||
           name.find("compare") != std::string_view::npos || name.find("equal") != std::string_view::npos;
}

class FieldPairVisitor : public ASTVisitor<FieldPairVisitor, VisitFlags::AllGood> {
public:
    FieldPairVisitor(std::string_view method, core::DiagnosticEngine& diags) : method(method), diags(diags) {}

    void handle(const AssignmentExpression& a) {
        if (!a.isLValueArg()) check(a.left(), a.right(), a);
        visitDefault(a);
    }

    void handle(const BinaryExpression& b) {
        switch (b.op) {
            case BinaryOperator::Equality: case BinaryOperator::Inequality:
            case BinaryOperator::CaseEquality: case BinaryOperator::CaseInequality:
                check(b.left(), b.right(), b);
                check(b.right(), b.left(), b);
                break;
            default:
                break;
        }
        visitDefault(b);
    }

    void handle(const CallExpression& call) {
        // comparer.compare_field("addr", addr, rhs_.data, 32) and similar helpers.
        const Expression* own = nullptr;
        const Expression* other = nullptr;
        int owns = 0, others = 0;
        for (const auto* arg : call.arguments()) {
            if (!arg) continue;
            if (ownField(*arg)) { own = arg; owns++; }
            else if (otherField(*arg).first) { other = arg; others++; }
        }
        if (owns == 1 && others == 1) check(*own, *other, call);
        visitDefault(call);
    }

private:
    std::string_view method;
    core::DiagnosticEngine& diags;

    void check(const Expression& mine, const Expression& theirs, const Expression& at) {
        const Symbol* a = ownField(mine);
        auto [b, otherCls] = otherField(theirs);
        if (!a || !b || a->name == b->name) return;
        if (!classHasField(otherCls, a->name)) return;
        diags.addViolation(
            "SV-BUG-004", "MismatchedFieldInCopyCompare",
            "In " + std::string(method) + "(), field '" + std::string(a->name) + "' is paired with '" +
                swe::exprText(theirs) + "' instead of the matching field '" + std::string(a->name) +
                "'. Field-wise copy and compare code is usually pasted line by line; check this line.",
            core::Severity::Error, swe::locOf(at));
    }
};

} // namespace

class MismatchedFieldInCopyCompare : public Rule {
public:
    std::string_view getId() const override { return "SV-BUG-004"; }
    std::string_view getName() const override { return "MismatchedFieldInCopyCompare"; }
    std::string_view getDescription() const override {
        return "In do_copy/do_compare and similar field-wise methods, flags a field of this object "
               "paired with a differently named field of the other object when the matching field exists.";
    }
    std::string_view getCategory() const override { return "Correctness"; }
    core::Severity getDefaultSeverity() const override { return core::Severity::Error; }

    void run(Compilation&, const uvm::UvmModel& model, core::DiagnosticEngine& diags) override {
        for (const auto* cls : swe::userClasses(model)) {
            for (const auto* m : swe::classMethods(*cls)) {
                if (!isFieldwiseMethod(m->name)) continue;
                FieldPairVisitor v(m->name, diags);
                m->getBody().visit(v);
            }
        }
    }
};

} // namespace uvmcheck::rules::correctness

namespace uvmcheck::rules {
std::unique_ptr<Rule> createMismatchedFieldInCopyCompare() {
    return std::make_unique<correctness::MismatchedFieldInCopyCompare>();
}
}
