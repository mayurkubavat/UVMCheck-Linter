#include "uvmcheck/rules/Rule.h"
#include "uvmcheck/rules/RuleRegistry.h"
#include "uvmcheck/rules/SweSupport.h"
#include "slang/ast/expressions/OperatorExpressions.h"
#include "slang/ast/statements/ConditionalStatements.h"

namespace uvmcheck::rules::design {

using namespace slang::ast;

namespace {

bool isTypeNameCall(const Expression& expr) {
    const Expression& e = swe::stripConversions(expr);
    if (e.kind != ExpressionKind::Call) return false;
    std::string_view n = e.as<CallExpression>().getSubroutineName();
    return n == "get_type_name";
}

bool isStringLiteral(const Expression& expr) {
    return swe::stripConversions(expr).kind == ExpressionKind::StringLiteral;
}

class TypeSwitchVisitor : public swe::UserCodeVisitor<TypeSwitchVisitor> {
public:
    using UserCodeVisitor::handle;

    explicit TypeSwitchVisitor(core::DiagnosticEngine& diags) : diags(diags) {}

    void handle(const BinaryExpression& b) {
        if (b.op == BinaryOperator::Equality || b.op == BinaryOperator::Inequality ||
            b.op == BinaryOperator::CaseEquality || b.op == BinaryOperator::CaseInequality) {
            if ((isTypeNameCall(b.left()) && isStringLiteral(b.right())) ||
                (isTypeNameCall(b.right()) && isStringLiteral(b.left()))) {
                report(b);
            }
        }
        visitDefault(b);
    }

    void handle(const CaseStatement& cs) {
        if (isTypeNameCall(cs.expr)) report(cs.expr);
        visitDefault(cs);
    }

private:
    core::DiagnosticEngine& diags;

    void report(const Expression& at) {
        diags.addViolation(
            "SV-DSN-001", "TypeSwitchByName",
            "Behavior is chosen by comparing get_type_name() with a string. Every new type then needs "
            "edits here, and a renamed class silently falls through. Move the behavior into a virtual "
            "method of the class hierarchy (open/closed principle).",
            core::Severity::Warning, swe::locOf(at));
    }
};

} // namespace

class TypeSwitchByName : public Rule {
public:
    std::string_view getId() const override { return "SV-DSN-001"; }
    std::string_view getName() const override { return "TypeSwitchByName"; }
    std::string_view getDescription() const override {
        return "Flags branching on get_type_name() compared with string literals instead of virtual "
               "dispatch (Sonar 'instanceof chains', open/closed principle).";
    }
    std::string_view getCategory() const override { return "Architecture"; }
    core::Severity getDefaultSeverity() const override { return core::Severity::Warning; }

    void run(Compilation& compilation, const uvm::UvmModel&, core::DiagnosticEngine& diags) override {
        TypeSwitchVisitor v(diags);
        compilation.getRoot().visit(v);
    }
};

} // namespace uvmcheck::rules::design

namespace uvmcheck::rules {
std::unique_ptr<Rule> createTypeSwitchByName() {
    return std::make_unique<design::TypeSwitchByName>();
}
}
