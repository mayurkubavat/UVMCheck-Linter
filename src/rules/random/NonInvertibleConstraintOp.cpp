#include "uvmcheck/rules/Rule.h"
#include "uvmcheck/rules/RuleRegistry.h"
#include "slang/ast/ASTVisitor.h"
#include "slang/ast/symbols/ClassSymbols.h"
#include "slang/ast/expressions/Operator.h"
#include "slang/ast/Constraints.h"

namespace uvmcheck::rules::random {

class NonInvertibleConstraintVisitor : public slang::ast::ASTVisitor<NonInvertibleConstraintVisitor, slang::ast::VisitFlags::AllGood> {
public:
    explicit NonInvertibleConstraintVisitor(core::DiagnosticEngine& diags, std::string_view cblockName)
        : diags(diags), cblockName(cblockName) {}

    void handle(const slang::ast::BinaryExpression& expr) {
        if (expr.op == slang::ast::BinaryOperator::Mod ||
            expr.op == slang::ast::BinaryOperator::Divide ||
            expr.op == slang::ast::BinaryOperator::Power) {

            std::string opStr;
            if (expr.op == slang::ast::BinaryOperator::Mod) opStr = "% (modulo)";
            else if (expr.op == slang::ast::BinaryOperator::Divide) opStr = "/ (division)";
            else opStr = "** (exponentiation)";

            diags.addViolation(
                "SV-RND-008",
                "NonInvertibleConstraintOp",
                "Non-invertible mathematical operator '" + opStr + "' used inside constraint block '" +
                std::string(cblockName) + "'. Non-linear arithmetic cannot be mapped efficiently to 2-state SAT/SMT solver equations, triggering exponential backtracking and simulator stalls.",
                core::Severity::Warning,
                expr.sourceRange.start()
            );
        }
        visitDefault(expr);
    }

private:
    core::DiagnosticEngine& diags;
    std::string_view cblockName;
};

class NonInvertibleConstraintOp : public Rule {
public:
    std::string_view getId() const override { return "SV-RND-008"; }
    std::string_view getName() const override { return "NonInvertibleConstraintOp"; }
    std::string_view getDescription() const override {
        return "Disallow non-invertible operators (%, /, **) inside constraint blocks. "
               "Non-linear operators prevent SAT/SMT solvers from bi-directionally pruning solution spaces, "
               "causing catastrophic backtracking and long simulation stalls.";
    }
    std::string_view getCategory() const override { return "Randomization"; }
    core::Severity getDefaultSeverity() const override { return core::Severity::Warning; }

    void run(slang::ast::Compilation&,
             const uvmcheck::uvm::UvmModel& uvmModel,
             core::DiagnosticEngine& diags) override {
        for (const auto* cls : uvmModel.getAllClasses()) {
            if (!cls) continue;

            for (const auto& member : cls->members()) {
                if (member.kind == slang::ast::SymbolKind::ConstraintBlock) {
                    const auto& cblock = member.as<slang::ast::ConstraintBlockSymbol>();
                    NonInvertibleConstraintVisitor visitor(diags, cblock.name);
                    cblock.visitExprs(visitor);
                }
            }
        }
    }
};

} // namespace uvmcheck::rules::random

namespace uvmcheck::rules {
std::unique_ptr<Rule> createNonInvertibleConstraintOp() {
    return std::make_unique<random::NonInvertibleConstraintOp>();
}
}
