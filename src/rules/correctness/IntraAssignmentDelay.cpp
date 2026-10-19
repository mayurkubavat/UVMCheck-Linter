#include "uvmcheck/rules/Rule.h"
#include "uvmcheck/rules/RuleRegistry.h"
#include "slang/syntax/AllSyntax.h"
#include "slang/syntax/SyntaxTree.h"
#include "slang/syntax/SyntaxVisitor.h"

namespace uvmcheck::rules::correctness {

class IntraAssignmentDelayVisitor : public slang::syntax::SyntaxVisitor<IntraAssignmentDelayVisitor> {
public:
    explicit IntraAssignmentDelayVisitor(core::DiagnosticEngine& diags) : diags(diags) {}

    void handle(const slang::syntax::BinaryExpressionSyntax& expr) {
        if (expr.kind == slang::syntax::SyntaxKind::AssignmentExpression &&
            expr.right->kind == slang::syntax::SyntaxKind::TimingControlExpression) {
            if (expr.operatorToken.kind == slang::parsing::TokenKind::Equals) {
                diags.addViolation(
                    "SV-DLY-002",
                    "IntraAssignmentDelay",
                    "Intra-assignment delay used on blocking procedural assignment ('='). "
                    "Blocking intra-assignment delays (e.g. 'a = #5 b;') sample the RHS, suspend thread execution for the delay, and update the LHS later. "
                    "This creates severe simulation race conditions if RHS signals mutate in the interim. "
                    "Use separate procedural delay statements ('#5; a = b;') or non-blocking clocking drives.",
                    core::Severity::Error,
                    expr.operatorToken.location()
                );
            } else if (expr.operatorToken.kind == slang::parsing::TokenKind::LessThanEquals) {
                diags.addViolation(
                    "SV-DLY-002",
                    "IntraAssignmentDelay",
                    "Intra-assignment delay used on non-blocking assignment ('<='). "
                    "Intra-assignment timing controls are non-synthesizable and complicate timing verification. "
                    "Use explicit procedural delays or clocking block output skew specifications instead.",
                    core::Severity::Warning,
                    expr.operatorToken.location()
                );
            }
        }
        visitDefault(expr);
    }

private:
    core::DiagnosticEngine& diags;
};

class IntraAssignmentDelay : public Rule {
public:
    std::string_view getId() const override { return "SV-DLY-002"; }
    std::string_view getName() const override { return "IntraAssignmentDelay"; }
    std::string_view getDescription() const override {
        return "Disallows intra-assignment timing controls (e.g., a = #5 b; or a <= #5 b;) on procedural assignments. "
               "Intra-assignment delays cause simulation race conditions and are non-synthesizable.";
    }
    std::string_view getCategory() const override { return "Correctness"; }
    core::Severity getDefaultSeverity() const override { return core::Severity::Error; }

    void run(slang::ast::Compilation& compilation,
             const uvmcheck::uvm::UvmModel& /*uvmModel*/,
             core::DiagnosticEngine& diags) override {
        IntraAssignmentDelayVisitor visitor(diags);
        for (const auto& tree : compilation.getSyntaxTrees()) {
            if (tree) {
                tree->root().visit(visitor);
            }
        }
    }
};

} // namespace uvmcheck::rules::correctness

namespace uvmcheck::rules {
std::unique_ptr<Rule> createIntraAssignmentDelay() {
    return std::make_unique<correctness::IntraAssignmentDelay>();
}
}
