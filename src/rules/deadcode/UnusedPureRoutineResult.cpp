#include "uvmcheck/rules/Rule.h"
#include "uvmcheck/rules/RuleRegistry.h"
#include "slang/syntax/AllSyntax.h"
#include "slang/syntax/SyntaxTree.h"
#include "slang/syntax/SyntaxVisitor.h"

namespace uvmcheck::rules::deadcode {

class UnusedPureRoutineVisitor : public engine::SyntaxVisitor<UnusedPureRoutineVisitor> {
public:
    explicit UnusedPureRoutineVisitor(core::DiagnosticEngine& diags) : diags(diags) {}

    void handle(const slang::syntax::ExpressionStatementSyntax& stmt) {
        if (stmt.expr->kind == slang::syntax::SyntaxKind::InvocationExpression) {
            const auto& inv = stmt.expr->as<slang::syntax::InvocationExpressionSyntax>();
            checkInvocation(inv);
        }
        visitDefault(stmt);
    }

private:
    core::DiagnosticEngine& diags;

    void checkInvocation(const slang::syntax::InvocationExpressionSyntax& inv) {
        std::string_view name = inv.left->getLastToken().valueText();

        if (name == "$sformatf") {
            diags.addViolation(
                "SV-DED-007",
                "UnusedPureRoutineResult",
                "Result of pure system function '$sformatf()' is discarded in a standalone statement. "
                "In SystemVerilog, $sformatf() returns a formatted string without printing or modifying arguments. "
                "Use $display() to output to the console or $sformat(dest, ...) to write to a string variable.",
                core::Severity::Warning,
                inv.getFirstToken().location()
            );
        }
        else if (isPureSystemFunction(name)) {
            diags.addViolation(
                "SV-DED-007",
                "UnusedPureRoutineResult",
                "Result of value-returning system function '" + std::string(name) + "()' is discarded in a standalone statement.",
                core::Severity::Warning,
                inv.getFirstToken().location()
            );
        }
        else if (isPureMethod(name)) {
            diags.addViolation(
                "SV-DED-007",
                "UnusedPureRoutineResult",
                "Result of value-returning pure routine '." + std::string(name) + "()' is discarded in a standalone statement.",
                core::Severity::Warning,
                inv.left->getLastToken().location()
            );
        }
    }

    static bool isPureSystemFunction(std::string_view name) {
        return name == "$realtime" || name == "$time" || name == "$stime" ||
               name == "$clog2" || name == "$bits" || name == "$typename" ||
               name == "$sin" || name == "$cos" || name == "$tan" ||
               name == "$ln" || name == "$log10" || name == "$exp" ||
               name == "$sqrt" || name == "$pow";
    }

    static bool isPureMethod(std::string_view name) {
        return name == "len" || name == "substr" ||
               name == "toupper" || name == "tolower" ||
               name == "compare" || name == "icompare" ||
               name == "atoi" || name == "atohex" || name == "atobin" ||
               name == "atooct" || name == "atoreal" ||
               name == "sum" || name == "product";
    }
};

class UnusedPureRoutineResult : public Rule {
public:
    std::string_view getId() const override { return "SV-DED-007"; }
    std::string_view getName() const override { return "UnusedPureRoutineResult"; }
    std::string_view getDescription() const override {
        return "Flags calls to pure functions or value-returning routines ($sformatf, $realtime, len, toupper, sum) "
               "used as standalone statements where the result is discarded.";
    }
    std::string_view getCategory() const override { return "DeadCode"; }
    core::Severity getDefaultSeverity() const override { return core::Severity::Warning; }

    engine::RuleTask check(engine::RuleContext& ctx) override {
        auto& diags = ctx.diags();
        UnusedPureRoutineVisitor visitor(diags);
        co_await ctx.visitSyntax(visitor);
    }
};

} // namespace uvmcheck::rules::deadcode

namespace uvmcheck::rules {
std::unique_ptr<Rule> createUnusedPureRoutineResult() {
    return std::make_unique<deadcode::UnusedPureRoutineResult>();
}
}
