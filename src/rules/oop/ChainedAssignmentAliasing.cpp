#include "uvmcheck/rules/Rule.h"
#include "uvmcheck/rules/RuleRegistry.h"
#include "slang/syntax/AllSyntax.h"
#include "slang/syntax/SyntaxTree.h"
#include "slang/syntax/SyntaxVisitor.h"
#include <string>

namespace uvmcheck::rules::oop {

class ChainedAssignmentVisitor : public engine::SyntaxVisitor<ChainedAssignmentVisitor> {
public:
    explicit ChainedAssignmentVisitor(core::DiagnosticEngine& diags) : diags(diags) {}

    void handle(const slang::syntax::BinaryExpressionSyntax& bin) {
        if (bin.kind == slang::syntax::SyntaxKind::AssignmentExpression) {
            bool isChained = false;
            if (bin.left && bin.left->kind == slang::syntax::SyntaxKind::AssignmentExpression) {
                isChained = true;
            }
            if (bin.right && bin.right->kind == slang::syntax::SyntaxKind::AssignmentExpression) {
                isChained = true;
            }

            if (isChained) {
                diags.addViolation(
                    "SV-OOP-012",
                    "ChainedAssignmentAliasing",
                    "Chained assignment expression detected. "
                    "In SystemVerilog verification environments, chained assignments (e.g. 'a = b = c;') create shared object aliases "
                    "across multiple references without clone safety, leading to unintentional state mutations across threads. "
                    "Split chained assignments into discrete statements.",
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

class ChainedAssignmentAliasing : public Rule {
public:
    std::string_view getId() const override { return "SV-OOP-012"; }
    std::string_view getName() const override { return "ChainedAssignmentAliasing"; }
    std::string_view getDescription() const override {
        return "Disallows chained assignment expressions (e.g. a = b = c;). "
               "Chained assignments create dangerous reference aliasing across class handles.";
    }
    std::string_view getCategory() const override { return "OOP"; }
    core::Severity getDefaultSeverity() const override { return core::Severity::Warning; }

    engine::RuleTask check(engine::RuleContext& ctx) override {
        auto& diags = ctx.diags();
        ChainedAssignmentVisitor visitor(diags);
        co_await ctx.visitSyntax(visitor);
    }
};

} // namespace uvmcheck::rules::oop

namespace uvmcheck::rules {
std::unique_ptr<Rule> createChainedAssignmentAliasing() {
    return std::make_unique<oop::ChainedAssignmentAliasing>();
}
}
