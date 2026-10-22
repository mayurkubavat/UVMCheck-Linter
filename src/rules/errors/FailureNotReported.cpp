#include "uvmcheck/rules/Rule.h"
#include "uvmcheck/rules/RuleRegistry.h"
#include "uvmcheck/rules/SweSupport.h"
#include "slang/ast/expressions/OperatorExpressions.h"
#include "slang/ast/statements/ConditionalStatements.h"
#include "slang/ast/statements/MiscStatements.h"

namespace uvmcheck::rules::errors {

using namespace slang::ast;

namespace {

enum class StatusCall { None, Randomize, Cast, ConfigGet, TryOp, Compare };

const char* statusName(StatusCall s) {
    switch (s) {
        case StatusCall::Randomize: return "randomize()";
        case StatusCall::Cast: return "$cast()";
        case StatusCall::ConfigGet: return "uvm_config_db::get()";
        case StatusCall::TryOp: return "try_* call";
        case StatusCall::Compare: return "compare()";
        default: return "status check";
    }
}

StatusCall classify(const Expression& expr) {
    const Expression& e = swe::stripConversions(expr);
    if (e.kind != ExpressionKind::Call) return StatusCall::None;
    const auto& call = e.as<CallExpression>();
    std::string_view n = call.getSubroutineName();
    if (n == "randomize") return StatusCall::Randomize;
    if (n == "$cast") return StatusCall::Cast;
    if (n == "try_get" || n == "try_put" || n == "try_peek") return StatusCall::TryOp;
    if (n == "compare") return StatusCall::Compare;
    if (n == "get" && swe::exprText(call).find("uvm_config_db") != std::string::npos)
        return StatusCall::ConfigGet;
    return StatusCall::None;
}

// Returns the status call when `cond` tests for its failure (!call, call == 0).
StatusCall failureTest(const Expression& cond) {
    const Expression& e = swe::stripConversions(cond);
    if (e.kind == ExpressionKind::UnaryOp) {
        const auto& u = e.as<UnaryExpression>();
        if (u.op == UnaryOperator::LogicalNot) return classify(u.operand());
    }
    if (e.kind == ExpressionKind::BinaryOp) {
        const auto& b = e.as<BinaryExpression>();
        if (b.op == BinaryOperator::Equality || b.op == BinaryOperator::CaseEquality) {
            std::string r = swe::exprText(b.right());
            if (r == "0" || r == "1'b0" || r == "'0") return classify(b.left());
        }
    }
    return StatusCall::None;
}

// True when the branch does nothing except report at a level below error (or nothing at all).
bool onlyWeakReports(const Statement& stmt, swe::ReportLevel& strongest) {
    switch (stmt.kind) {
        case StatementKind::Empty:
            return true;
        case StatementKind::Block:
            return onlyWeakReports(stmt.as<BlockStatement>().body, strongest);
        case StatementKind::List:
            for (const auto* s : stmt.as<StatementList>().list) {
                if (s && !onlyWeakReports(*s, strongest)) return false;
            }
            return true;
        case StatementKind::Conditional: {
            // `uvm_* macros expand to a report guard around uvm_report_*(...).
            const auto& c = stmt.as<ConditionalStatement>();
            if (c.ifFalse || !swe::isReportGuard(c)) return false;
            return onlyWeakReports(c.ifTrue, strongest);
        }
        case StatementKind::VariableDeclaration:
            return swe::isReportScaffolding(stmt);
        case StatementKind::ExpressionStatement: {
            if (swe::isReportScaffolding(stmt)) return true;
            const Expression& e = swe::stripConversions(stmt.as<ExpressionStatement>().expr);
            if (e.kind != ExpressionKind::Call) return false;
            const auto& call = e.as<CallExpression>();
            auto level = swe::reportLevel(call);
            std::string_view n = call.getSubroutineName();
            if (level == swe::ReportLevel::None && n != "$display" && n != "$write") return false;
            if (level >= swe::ReportLevel::Error) return false;
            if (level > strongest) strongest = level;
            return true;
        }
        default:
            return false;
    }
}

class FailureNotReportedVisitor : public swe::UserCodeVisitor<FailureNotReportedVisitor> {
public:
    using UserCodeVisitor::handle;

    explicit FailureNotReportedVisitor(core::DiagnosticEngine& diags) : diags(diags) {}

    void handle(const ConditionalStatement& cond) {
        if (cond.conditions.size() == 1 && !cond.conditions[0].pattern) {
            const Expression& c = *cond.conditions[0].expr;
            if (StatusCall s = failureTest(c); s != StatusCall::None) {
                check(s, cond.ifTrue, cond);
            } else if (StatusCall p = classify(c); p != StatusCall::None && cond.ifFalse) {
                check(p, *cond.ifFalse, cond);
            }
        }
        visitDefault(cond);
    }

    void handle(const ImmediateAssertionStatement& assertion) {
        if (StatusCall s = classify(assertion.cond); s != StatusCall::None && assertion.ifFalse)
            check(s, *assertion.ifFalse, assertion);
        visitDefault(assertion);
    }

private:
    core::DiagnosticEngine& diags;

    void check(StatusCall s, const Statement& failBranch, const Statement& at) {
        swe::ReportLevel strongest = swe::ReportLevel::None;
        if (!onlyWeakReports(failBranch, strongest)) return;
        // A missing optional config_db entry reported as a warning is a common, valid choice.
        if (s == StatusCall::ConfigGet && strongest == swe::ReportLevel::Warning) return;
        std::string how = strongest == swe::ReportLevel::None ? "is silently ignored"
                        : strongest == swe::ReportLevel::Info ? "is only reported with `uvm_info"
                                                               : "is only reported as a warning";
        diags.addViolation(
            "SV-ERR-001", "FailureNotReported",
            std::string("Failure of ") + statusName(s) + " " + how +
                ", so the test still passes after detecting it. Report it with `uvm_error or "
                "`uvm_fatal, or handle the failure explicitly.",
            core::Severity::Error, swe::locOf(at));
    }
};

} // namespace

class FailureNotReported : public Rule {
public:
    std::string_view getId() const override { return "SV-ERR-001"; }
    std::string_view getName() const override { return "FailureNotReported"; }
    std::string_view getDescription() const override {
        return "Flags randomize(), $cast(), uvm_config_db::get(), try_* and compare() status checks "
               "whose failure branch is empty or only reports below error severity (Sonar S108/S1166).";
    }
    std::string_view getCategory() const override { return "Correctness"; }
    core::Severity getDefaultSeverity() const override { return core::Severity::Error; }

    void run(Compilation& compilation, const uvm::UvmModel&, core::DiagnosticEngine& diags) override {
        FailureNotReportedVisitor v(diags);
        compilation.getRoot().visit(v);
    }
};

} // namespace uvmcheck::rules::errors

namespace uvmcheck::rules {
std::unique_ptr<Rule> createFailureNotReported() {
    return std::make_unique<errors::FailureNotReported>();
}
}
