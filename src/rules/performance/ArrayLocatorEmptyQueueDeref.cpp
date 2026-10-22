#include "uvmcheck/rules/Rule.h"
#include "uvmcheck/rules/RuleRegistry.h"
#include "slang/syntax/AllSyntax.h"
#include "slang/syntax/SyntaxTree.h"
#include "slang/syntax/SyntaxVisitor.h"

namespace uvmcheck::rules::performance {

class ArrayLocatorDerefVisitor : public engine::SyntaxVisitor<ArrayLocatorDerefVisitor> {
public:
    explicit ArrayLocatorDerefVisitor(core::DiagnosticEngine& diags) : diags(diags) {}

    void handle(const slang::syntax::ElementSelectExpressionSyntax& expr) {
        std::string locatorName = getLocatorMethodName(*expr.left);
        if (!locatorName.empty()) {
            diags.addViolation(
                "SV-LOC-001",
                "ArrayLocatorEmptyQueueDeref",
                "Array locator method '" + locatorName + "()' returns an unpacked queue which is immediately indexed with '[...]' without checking for emptiness. If no elements match, an empty queue is returned and accessing an index causes an out-of-bounds error per IEEE 1800 § 7.12.1. Store the locator result in a queue and check 'if (res.size() > 0)' before dereferencing.",
                core::Severity::Error,
                expr.select->openBracket.location()
            );
        }
        visitDefault(expr);
    }

private:
    core::DiagnosticEngine& diags;

    static std::string getLocatorMethodName(const slang::syntax::ExpressionSyntax& expr) {
        if (expr.kind == slang::syntax::SyntaxKind::ParenthesizedExpression) {
            const auto& paren = expr.as<slang::syntax::ParenthesizedExpressionSyntax>();
            return getLocatorMethodName(*paren.expression);
        }
        if (expr.kind == slang::syntax::SyntaxKind::ArrayOrRandomizeMethodExpression) {
            const auto& arrMethod = expr.as<slang::syntax::ArrayOrRandomizeMethodExpressionSyntax>();
            return getLocatorMethodName(*arrMethod.method);
        }
        if (expr.kind == slang::syntax::SyntaxKind::InvocationExpression) {
            const auto& inv = expr.as<slang::syntax::InvocationExpressionSyntax>();
            std::string_view name = inv.left->getLastToken().valueText();
            if (isLocatorMethod(name)) {
                return std::string(name);
            }
        }
        std::string_view name = expr.getLastToken().valueText();
        if (isLocatorMethod(name)) {
            return std::string(name);
        }
        return "";
    }

    static bool isLocatorMethod(std::string_view name) {
        return name == "find" || name == "find_index" ||
               name == "find_first" || name == "find_first_index" ||
               name == "find_last" || name == "find_last_index" ||
               name == "min" || name == "max" ||
               name == "unique" || name == "unique_index";
    }
};

class ArrayLocatorEmptyQueueDeref : public Rule {
public:
    std::string_view getId() const override { return "SV-LOC-001"; }
    std::string_view getName() const override { return "ArrayLocatorEmptyQueueDeref"; }
    std::string_view getDescription() const override {
        return "Disallows immediate element dereferencing on array locator methods (e.g., q.find()[0]). "
               "Locator methods return an unpacked queue that may be empty, causing out-of-bounds runtime errors (IEEE 1800 § 7.12.1).";
    }
    std::string_view getCategory() const override { return "Performance"; }
    core::Severity getDefaultSeverity() const override { return core::Severity::Error; }

    engine::RuleTask check(engine::RuleContext& ctx) override {
        auto& diags = ctx.diags();
        ArrayLocatorDerefVisitor visitor(diags);
        co_await ctx.visitSyntax(visitor);
    }
};

} // namespace uvmcheck::rules::performance

namespace uvmcheck::rules {
std::unique_ptr<Rule> createArrayLocatorEmptyQueueDeref() {
    return std::make_unique<performance::ArrayLocatorEmptyQueueDeref>();
}
}
