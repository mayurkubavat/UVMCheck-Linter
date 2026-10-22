#include "uvmcheck/rules/Rule.h"
#include "uvmcheck/rules/RuleRegistry.h"
#include "uvmcheck/rules/SweSupport.h"
#include "slang/ast/expressions/ConversionExpression.h"
#include "slang/ast/expressions/OperatorExpressions.h"

namespace uvmcheck::rules::correctness {

using namespace slang::ast;

class IntegerDivisionToRealVisitor : public swe::UserCodeVisitor<IntegerDivisionToRealVisitor> {
public:
    using UserCodeVisitor::handle;

    explicit IntegerDivisionToRealVisitor(core::DiagnosticEngine& diags) : diags(diags) {}

    void handle(const ConversionExpression& conv) {
        if (conv.isImplicit() && conv.type->isFloating()) {
            const Expression& op = swe::stripConversions(conv.operand());
            if (op.kind == ExpressionKind::BinaryOp) {
                const auto& bin = op.as<BinaryExpression>();
                if (bin.op == BinaryOperator::Divide && bin.type->isIntegral() &&
                    bin.left().type->isIntegral() && bin.right().type->isIntegral()) {
                    diags.addViolation(
                        "SV-BUG-005", "IntegerDivisionToReal",
                        "'" + swe::exprText(bin) + "' divides two integers, so the fraction is dropped "
                            "before the result becomes real. Convert an operand first, for example "
                            "'real'(" + swe::exprText(bin.left()) + ") / " + swe::exprText(bin.right()) + "'.",
                        core::Severity::Warning, swe::locOf(bin));
                }
            }
        }
        visitDefault(conv);
    }

private:
    core::DiagnosticEngine& diags;
};

class IntegerDivisionToReal : public Rule {
public:
    std::string_view getId() const override { return "SV-BUG-005"; }
    std::string_view getName() const override { return "IntegerDivisionToReal"; }
    std::string_view getDescription() const override {
        return "Flags integer divisions whose result is used as a real value, which truncates before "
               "the conversion (Sonar S2184, SpotBugs ICAST_IDIV_CAST_TO_DOUBLE).";
    }
    std::string_view getCategory() const override { return "Correctness"; }
    core::Severity getDefaultSeverity() const override { return core::Severity::Warning; }

    void run(Compilation& compilation, const uvm::UvmModel&, core::DiagnosticEngine& diags) override {
        IntegerDivisionToRealVisitor v(diags);
        compilation.getRoot().visit(v);
    }
};

} // namespace uvmcheck::rules::correctness

namespace uvmcheck::rules {
std::unique_ptr<Rule> createIntegerDivisionToReal() {
    return std::make_unique<correctness::IntegerDivisionToReal>();
}
}
