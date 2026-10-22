#include "uvmcheck/rules/Rule.h"
#include "uvmcheck/rules/RuleRegistry.h"
#include "slang/syntax/AllSyntax.h"
#include "slang/syntax/SyntaxTree.h"
#include "slang/syntax/SyntaxVisitor.h"
#include <string>
#include <algorithm>

namespace uvmcheck::rules::performance {

class ArraySizeConstraintVisitor : public engine::SyntaxVisitor<ArraySizeConstraintVisitor> {
public:
    explicit ArraySizeConstraintVisitor(core::DiagnosticEngine& diags) : diags(diags) {}

    void handle(const slang::syntax::ExpressionConstraintSyntax& citem) {
        if (citem.expr) {
            std::string exprStr = std::string(citem.expr->toString());
            if (size_t lastNl = exprStr.rfind('\n'); lastNl != std::string::npos) {
                exprStr = exprStr.substr(lastNl + 1);
            }
            exprStr.erase(0, exprStr.find_first_not_of(" \t\r"));
            if (size_t end = exprStr.find_last_not_of(" \t\r"); end != std::string::npos) {
                exprStr.erase(end + 1);
            }
            if (exprStr.find(".size()") != std::string::npos) {
                bool hasUpperBound = (exprStr.find('<') != std::string::npos);
                bool hasLowerBound = (exprStr.find('>') != std::string::npos ||
                                      exprStr.find("inside") != std::string::npos ||
                                      exprStr.find("==") != std::string::npos);

                if (hasUpperBound && !hasLowerBound) {
                    // Clean up display string
                    exprStr.erase(std::remove(exprStr.begin(), exprStr.end(), '\n'), exprStr.end());
                    exprStr.erase(std::remove(exprStr.begin(), exprStr.end(), '\r'), exprStr.end());

                    diags.addViolation(
                        "SV-PRF-012",
                        "UnboundedDynamicArrayInConstraint",
                        "Dynamic array size constraint '" + exprStr +
                        "' specifies an upper bound without an explicit non-negative lower bound (e.g. 'size() >= 0' or 'size() > 0'). "
                        "In SystemVerilog constraint optimization (IEEE 1800 § 18.5.1), "
                        "solvers must search full 32-bit signed integer negative spaces when lower bounds are omitted, resulting in "
                        "exponential solver search space explosion. Constrain both bounds explicitly (e.g. 'size() inside {[1:10]}').",
                        core::Severity::Warning,
                        citem.expr->getFirstToken().location()
                    );
                }
            }
        }
        visitDefault(citem);
    }

private:
    core::DiagnosticEngine& diags;
};

class UnboundedDynamicArrayInConstraint : public Rule {
public:
    std::string_view getId() const override { return "SV-PRF-012"; }
    std::string_view getName() const override { return "UnboundedDynamicArrayInConstraint"; }
    std::string_view getDescription() const override {
        return "Flags dynamic array or queue size constraints with an upper bound (< or <=) lacking an explicit lower bound.";
    }
    std::string_view getCategory() const override { return "Performance"; }
    core::Severity getDefaultSeverity() const override { return core::Severity::Warning; }

    engine::RuleTask check(engine::RuleContext& ctx) override {
        auto& diags = ctx.diags();
        ArraySizeConstraintVisitor visitor(diags);
        co_await ctx.visitSyntax(visitor);
    }
};

} // namespace uvmcheck::rules::performance

namespace uvmcheck::rules {
std::unique_ptr<Rule> createUnboundedDynamicArrayInConstraint() {
    return std::make_unique<performance::UnboundedDynamicArrayInConstraint>();
}
} // namespace uvmcheck::rules
