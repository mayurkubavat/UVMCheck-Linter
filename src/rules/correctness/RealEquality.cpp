#include "uvmcheck/rules/Rule.h"
#include "uvmcheck/rules/RuleRegistry.h"
#include "uvmcheck/rules/SweSupport.h"
#include "slang/ast/expressions/LiteralExpressions.h"
#include "slang/ast/expressions/OperatorExpressions.h"

namespace uvmcheck::rules::correctness {

using namespace slang::ast;

namespace {

bool isZeroLiteral(const Expression& e) {
    const Expression& x = swe::stripConversions(e);
    if (x.kind == ExpressionKind::RealLiteral) return x.as<RealLiteral>().getValue() == 0.0;
    if (x.kind == ExpressionKind::IntegerLiteral) {
        auto v = x.as<IntegerLiteral>().getValue().as<int64_t>();
        return v && *v == 0;
    }
    return false;
}

} // namespace

class RealEqualityVisitor : public swe::UserCodeVisitor<RealEqualityVisitor> {
public:
    using UserCodeVisitor::handle;

    explicit RealEqualityVisitor(core::DiagnosticEngine& diags) : diags(diags) {}

    void handle(const BinaryExpression& bin) {
        bool eq = bin.op == BinaryOperator::Equality || bin.op == BinaryOperator::Inequality ||
                  bin.op == BinaryOperator::CaseEquality || bin.op == BinaryOperator::CaseInequality;
        if (eq && (bin.left().type->isFloating() || bin.right().type->isFloating()) &&
            !isZeroLiteral(bin.left()) && !isZeroLiteral(bin.right())) {
            diags.addViolation(
                "SV-BUG-006", "RealEquality",
                "'" + swe::exprText(bin) + "' compares real values for exact equality. Rounding makes "
                    "this fail for values that are equal in practice; compare the difference against a "
                    "tolerance instead.",
                core::Severity::Warning, swe::locOf(bin));
        }
        visitDefault(bin);
    }

private:
    core::DiagnosticEngine& diags;
};

class RealEquality : public Rule {
public:
    std::string_view getId() const override { return "SV-BUG-006"; }
    std::string_view getName() const override { return "RealEquality"; }
    std::string_view getDescription() const override {
        return "Flags == and != on real or shortreal operands, except against an exact zero "
               "(Sonar S1244, clang-tidy float equality).";
    }
    std::string_view getCategory() const override { return "Correctness"; }
    core::Severity getDefaultSeverity() const override { return core::Severity::Warning; }

    void run(Compilation& compilation, const uvm::UvmModel&, core::DiagnosticEngine& diags) override {
        RealEqualityVisitor v(diags);
        compilation.getRoot().visit(v);
    }
};

} // namespace uvmcheck::rules::correctness

namespace uvmcheck::rules {
std::unique_ptr<Rule> createRealEquality() {
    return std::make_unique<correctness::RealEquality>();
}
}
