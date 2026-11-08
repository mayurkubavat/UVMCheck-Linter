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
                            // An item of another type ('byte' into an 'int' queue) is an implicit
                            // conversion, which has no syntax of its own: use its operand's.
                            while (!itemExpr->syntax && itemExpr->kind == slang::ast::ExpressionKind::Conversion)
                                itemExpr = &itemExpr->as<slang::ast::ConversionExpression>().operand();

                            std::vector<core::FixItHint> fixIts;
                            if (assign.sourceRange.start().valid() && itemExpr->syntax) {
                                // toString() keeps the token's leading trivia: "{q, x}" gives " x".
                                std::string itemText = std::string(itemExpr->syntax->toString());
                                auto first = itemText.find_first_not_of(" \t\r\n");
                                auto last = itemText.find_last_not_of(" \t\r\n");
                                itemText = first == std::string::npos ? "" : itemText.substr(first, last - first + 1);
                                if (!itemText.empty()) {
                                    std::string replacement = std::string(lhsSym->name) + "." + methodName + "(" + itemText + ")";
                                    core::FixItHint fix{assign.sourceRange, replacement, "Replace concatenation with " + methodName + "()"};
                                    // 'q <= {q, x}' updates q in the NBA region; push_back() updates it now.
                                    if (assign.isNonBlocking()) fix.safety = core::FixSafety::Unsafe;
                                    fixIts.push_back(std::move(fix));
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

UVMCHECK_REGISTER_RULE(uvmcheck::rules::createQueueConcatAppend)
