#include "uvmcheck/rules/Rule.h"
#include "uvmcheck/rules/RuleRegistry.h"
#include "slang/ast/ASTVisitor.h"
#include "slang/ast/expressions/CallExpression.h"
#include "slang/ast/expressions/ConversionExpression.h"
#include "slang/ast/statements/MiscStatements.h"

namespace uvmcheck::rules::random {

class UncheckedRandomizeVisitor : public slang::ast::ASTVisitor<UncheckedRandomizeVisitor, slang::ast::VisitFlags::AllGood> {
public:
    explicit UncheckedRandomizeVisitor(core::DiagnosticEngine& diags) : diags(diags) {}

    void handle(const slang::ast::ExpressionStatement& stmt) {
        const auto* expr = &stmt.expr;
        while (expr->kind == slang::ast::ExpressionKind::Conversion) {
            expr = &expr->as<slang::ast::ConversionExpression>().operand();
        }

        if (expr->kind == slang::ast::ExpressionKind::Call) {
            const auto& call = expr->as<slang::ast::CallExpression>();
            if (call.getSubroutineName() == "randomize") {
                diags.addViolation(
                    "SV-RND-001",
                    "UncheckedRandomize",
                    "Return value of 'randomize()' is discarded or cast to void. If constraint solving fails, "
                    "simulation proceeds silently with corrupt, unconstrained, or stale transaction data. "
                    "Check the return value with 'if (!obj.randomize())' or 'assert(obj.randomize())'.",
                    core::Severity::Error,
                    call.sourceRange.start()
                );
            }
        }
        visitDefault(stmt);
    }

private:
    core::DiagnosticEngine& diags;
};

class UncheckedRandomize : public Rule {
public:
    std::string_view getId() const override { return "SV-RND-001"; }
    std::string_view getName() const override { return "UncheckedRandomize"; }
    std::string_view getDescription() const override {
        return "The return value of randomize() must be checked. Silently ignoring failure leaves "
               "transactions un-randomized or corrupt, concealing verification testbench bugs.";
    }
    std::string_view getCategory() const override { return "Safety"; }
    core::Severity getDefaultSeverity() const override { return core::Severity::Error; }

    void run(slang::ast::Compilation& compilation,
             const uvmcheck::uvm::UvmModel& /*uvmModel*/,
             core::DiagnosticEngine& diags) override {
        UncheckedRandomizeVisitor visitor(diags);
        compilation.getRoot().visit(visitor);
    }
};

} // namespace uvmcheck::rules::random

namespace uvmcheck::rules {
std::unique_ptr<Rule> createUncheckedRandomize() {
    return std::make_unique<random::UncheckedRandomize>();
}
}
