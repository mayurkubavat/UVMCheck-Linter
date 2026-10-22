#include "uvmcheck/rules/Rule.h"
#include "uvmcheck/rules/RuleRegistry.h"
#include "slang/syntax/AllSyntax.h"
#include "slang/syntax/SyntaxTree.h"
#include "slang/syntax/SyntaxVisitor.h"
#include <string>

namespace uvmcheck::rules::correctness {

class AssignmentSubExprVisitor : public engine::SyntaxVisitor<AssignmentSubExprVisitor> {
public:
    explicit AssignmentSubExprVisitor(core::DiagnosticEngine& diags) : diags(diags) {}

    void handle(const slang::syntax::BinaryExpressionSyntax& bin) {
        if (bin.kind == slang::syntax::SyntaxKind::AssignmentExpression ||
            bin.kind == slang::syntax::SyntaxKind::AddAssignmentExpression ||
            bin.kind == slang::syntax::SyntaxKind::SubtractAssignmentExpression) {

            // Check if this assignment expression is embedded inside an outer expression
            const auto* curr = bin.parent.get();
            while (curr && curr->kind == slang::syntax::SyntaxKind::ParenthesizedExpression) {
                curr = curr->parent.get();
            }

            bool isStandalone = false;
            if (curr && (curr->kind == slang::syntax::SyntaxKind::ExpressionStatement ||
                         curr->kind == slang::syntax::SyntaxKind::ForLoopStatement ||
                         curr->kind == slang::syntax::SyntaxKind::CoverageOption)) {
                isStandalone = true;
            }

            if (!isStandalone) {
                std::string exprStr = std::string(bin.toString());
                if (size_t lastNl = exprStr.rfind('\n'); lastNl != std::string::npos) {
                    exprStr = exprStr.substr(lastNl + 1);
                }
                exprStr.erase(0, exprStr.find_first_not_of(" \t\r"));
                if (size_t end = exprStr.find_last_not_of(" \t\r"); end != std::string::npos) {
                    exprStr.erase(end + 1);
                }

                diags.addViolation(
                    "SV-COR-020",
                    "ProceduralAssignmentAsSubExpression",
                    "Assignment expression '" + exprStr +
                    "' is embedded as a sub-expression. In SystemVerilog (IEEE 1800 § 10.4), "
                    "assignments should stand alone as dedicated statements rather than being embedded in conditionals "
                    "or arithmetic expressions, which creates subtle order-of-evaluation race bugs and severely degrades code maintainability.",
                    core::Severity::Warning,
                    bin.operatorToken.location()
                );
            }
        }
        visitDefault(bin);
    }

private:
    core::DiagnosticEngine& diags;
};

class ProceduralAssignmentAsSubExpression : public Rule {
public:
    std::string_view getId() const override { return "SV-COR-020"; }
    std::string_view getName() const override { return "ProceduralAssignmentAsSubExpression"; }
    std::string_view getDescription() const override {
        return "Disallows embedded assignment expressions used as sub-expressions within conditionals or arithmetic operations.";
    }
    std::string_view getCategory() const override { return "Correctness"; }
    core::Severity getDefaultSeverity() const override { return core::Severity::Warning; }

    engine::RuleTask check(engine::RuleContext& ctx) override {
        auto& diags = ctx.diags();
        AssignmentSubExprVisitor visitor(diags);
        co_await ctx.visitSyntax(visitor);
    }
};

} // namespace uvmcheck::rules::correctness

namespace uvmcheck::rules {
std::unique_ptr<Rule> createProceduralAssignmentAsSubExpression() {
    return std::make_unique<correctness::ProceduralAssignmentAsSubExpression>();
}
} // namespace uvmcheck::rules
