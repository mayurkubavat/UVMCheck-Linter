#include "uvmcheck/rules/Rule.h"
#include "uvmcheck/rules/RuleRegistry.h"
#include "slang/ast/ASTVisitor.h"
#include "slang/ast/expressions/AssignmentExpressions.h"
#include "slang/ast/expressions/CallExpression.h"
#include "slang/ast/expressions/ConversionExpression.h"
#include "slang/syntax/AllSyntax.h"

namespace uvmcheck::rules::performance {

class ArrayReductionVisitor : public slang::ast::ASTVisitor<ArrayReductionVisitor, slang::ast::VisitFlags::AllGood> {
public:
    explicit ArrayReductionVisitor(core::DiagnosticEngine& diags) : diags(diags) {}

    void handle(const slang::ast::AssignmentExpression& assign) {
        const auto* rhs = &assign.right();
        while (rhs->kind == slang::ast::ExpressionKind::Conversion) {
            rhs = &rhs->as<slang::ast::ConversionExpression>().operand();
        }

        if (rhs->kind == slang::ast::ExpressionKind::Call) {
            const auto& call = rhs->as<slang::ast::CallExpression>();
            std::string_view name = call.getSubroutineName();

            if (name == "sum" || name == "product") {
                uint32_t lhsWidth = assign.left().type->getBitWidth();
                uint32_t elemWidth = call.type->getBitWidth();

                if (lhsWidth > elemWidth && elemWidth > 0) {
                    bool hasWithCast = false;
                    if (call.syntax) {
                        std::string syntaxStr(call.syntax->toString());
                        if (syntaxStr.find("with") != std::string::npos) {
                            hasWithCast = true;
                        }
                    }

                    if (!hasWithCast) {
                        diags.addViolation(
                            "SV-PRF-002",
                            "ArrayReductionWidthTruncation",
                            "Array reduction method '." + std::string(name) +
                            "()' accumulates using the array element width (" + std::to_string(elemWidth) +
                            " bits) before assignment to wider target (" + std::to_string(lhsWidth) +
                            " bits). Sum/product will silently wrap and truncate. Use with clause to expand accumulator, e.g. '.sum(item) with (int'(item))'.",
                            core::Severity::Error,
                            call.sourceRange.start()
                        );
                    }
                }
            }
        }
        visitDefault(assign);
    }

private:
    core::DiagnosticEngine& diags;
};

class ArrayReductionWidthTruncation : public Rule {
public:
    std::string_view getId() const override { return "SV-PRF-002"; }
    std::string_view getName() const override { return "ArrayReductionWidthTruncation"; }
    std::string_view getDescription() const override {
        return "Array reduction methods (.sum, .product) accumulate at element bit-width before extension to LHS. "
               "Summing sub-32-bit elements into 32/64-bit integers causes silent wraparound unless cast in with().";
    }
    std::string_view getCategory() const override { return "Performance"; }
    core::Severity getDefaultSeverity() const override { return core::Severity::Error; }

    void run(slang::ast::Compilation& compilation,
             const uvmcheck::uvm::UvmModel& /*uvmModel*/,
             core::DiagnosticEngine& diags) override {
        ArrayReductionVisitor visitor(diags);
        compilation.getRoot().visit(visitor);
    }
};

} // namespace uvmcheck::rules::performance

namespace uvmcheck::rules {
std::unique_ptr<Rule> createArrayReductionWidthTruncation() {
    return std::make_unique<performance::ArrayReductionWidthTruncation>();
}
}
