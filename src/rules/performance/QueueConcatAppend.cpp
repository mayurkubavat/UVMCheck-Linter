#include "uvmcheck/rules/Rule.h"
#include "uvmcheck/rules/RuleRegistry.h"
#include "slang/ast/ASTVisitor.h"
#include "slang/ast/expressions/AssignmentExpressions.h"
#include "slang/ast/expressions/ConversionExpression.h"
#include "slang/ast/expressions/OperatorExpressions.h"
#include "slang/syntax/AllSyntax.h"

namespace uvmcheck::rules::performance {

class QueueConcatVisitor : public engine::AstVisitor<QueueConcatVisitor> {
public:
    explicit QueueConcatVisitor(core::DiagnosticEngine& diags) : diags(diags) {}

    void handle(const slang::ast::AssignmentExpression& assign) {
        if (assign.left().type->isQueue()) {
            const auto* lhsSym = assign.left().getSymbolReference();
            if (lhsSym) {
                const auto* rhs = &assign.right();
                while (rhs->kind == slang::ast::ExpressionKind::Conversion) {
                    rhs = &rhs->as<slang::ast::ConversionExpression>().operand();
                }

                if (rhs->kind == slang::ast::ExpressionKind::Concatenation) {
                    auto operands = rhs->as<slang::ast::ConcatenationExpression>().operands();
                    if (operands.size() == 2) {
                        const auto* op0Sym = operands[0]->getSymbolReference();
                        const auto* op1Sym = operands[1]->getSymbolReference();

                        bool isPushBack = (op0Sym == lhsSym);
                        bool isPushFront = (op1Sym == lhsSym);

                        if (isPushBack || isPushFront) {
                            std::string methodName = isPushBack ? "push_back" : "push_front";
                            const auto* itemExpr = isPushBack ? operands[1] : operands[0];

                            std::vector<core::FixItHint> fixIts;
                            if (assign.sourceRange.start().valid() && itemExpr->syntax) {
                                std::string itemText = std::string(itemExpr->syntax->toString());
                                if (!itemText.empty()) {
                                    std::string replacement = std::string(lhsSym->name) + "." + methodName + "(" + itemText + ")";
                                    fixIts.push_back({assign.sourceRange, replacement, "Replace concatenation with " + methodName + "()"});
                                }
                            }

                            diags.addViolation(
                                "SV-PRF-007",
                                "QueueConcatAppend",
                                "Queue concatenation '" + std::string(lhsSym->name) + " = { ... }' causes O(N) memory reallocation and buffer copying on every operation. "
                                "In loops or transaction streams, this degrades into an O(N^2) memory churn bottleneck. "
                                "Use in-place '" + std::string(lhsSym->name) + "." + methodName + "(...)' instead for amortized O(1) performance.",
                                core::Severity::Warning,
                                assign.sourceRange.start(),
                                std::move(fixIts)
                            );
                        }
                    }
                }
            }
        }
        visitDefault(assign);
    }

private:
    core::DiagnosticEngine& diags;
};

class QueueConcatAppend : public Rule {
public:
    std::string_view getId() const override { return "SV-PRF-007"; }
    std::string_view getName() const override { return "QueueConcatAppend"; }
    std::string_view getDescription() const override {
        return "Disallow queue concatenation ('q = {q, x}' or 'q = {x, q}'). Concatenation reallocates memory on every append ($O(N^2)$ in loops); use push_back() or push_front() for amortized $O(1)$ performance.";
    }
    std::string_view getCategory() const override { return "Performance"; }
    core::Severity getDefaultSeverity() const override { return core::Severity::Warning; }

    engine::RuleTask check(engine::RuleContext& ctx) override {
        auto& diags = ctx.diags();
        QueueConcatVisitor visitor(diags);
        co_await ctx.visitAst(visitor);
    }
};

} // namespace uvmcheck::rules::performance

namespace uvmcheck::rules {
std::unique_ptr<Rule> createQueueConcatAppend() {
    return std::make_unique<performance::QueueConcatAppend>();
}
}
