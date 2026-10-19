#include "uvmcheck/rules/Rule.h"
#include "uvmcheck/rules/RuleRegistry.h"
#include "slang/ast/ASTVisitor.h"
#include "slang/ast/TimingControl.h"
#include "slang/ast/statements/MiscStatements.h"
#include "slang/ast/expressions/MiscExpressions.h"
#include "slang/syntax/AllSyntax.h"

namespace uvmcheck::rules::performance {

class ZeroDelayVisitor : public slang::ast::ASTVisitor<ZeroDelayVisitor, slang::ast::VisitFlags::AllGood> {
public:
    explicit ZeroDelayVisitor(core::DiagnosticEngine& diags) : diags(diags) {}

    void handle(const slang::ast::TimedStatement& stmt) {
        if (stmt.timing.kind == slang::ast::TimingControlKind::Delay) {
            const auto& delay = stmt.timing.as<slang::ast::DelayControl>();
            bool isZero = false;

            if (delay.expr.kind == slang::ast::ExpressionKind::IntegerLiteral) {
                if (delay.expr.as<slang::ast::IntegerLiteral>().getValue() == 0) {
                    isZero = true;
                }
            } else if (delay.expr.syntax && delay.expr.syntax->toString() == "0") {
                isZero = true;
            }

            if (isZero) {
                diags.addViolation(
                    "SV-PRF-004",
                    "ProceduralZeroDelay",
                    "Procedural delay '#0' reschedules the process into the Inactive simulation region. "
                    "Cascades of #0 delays cause delta-cycle thrashing, inflate event engine queues, and mask underlying race conditions. "
                    "Use non-blocking assignments (<=), clocking block skews, or explicit event synchronization instead.",
                    core::Severity::Warning,
                    stmt.sourceRange.start()
                );
            }
        }
        visitDefault(stmt);
    }

private:
    core::DiagnosticEngine& diags;
};

class ProceduralZeroDelay : public Rule {
public:
    std::string_view getId() const override { return "SV-PRF-004"; }
    std::string_view getName() const override { return "ProceduralZeroDelay"; }
    std::string_view getDescription() const override {
        return "Procedural delay #0 reschedules execution into the Inactive region. Repeated #0 rescheduling "
               "causes delta-cycle thrashing and degrades simulation throughput.";
    }
    std::string_view getCategory() const override { return "Performance"; }
    core::Severity getDefaultSeverity() const override { return core::Severity::Warning; }

    void run(slang::ast::Compilation& compilation,
             const uvmcheck::uvm::UvmModel&,
             core::DiagnosticEngine& diags) override {
        ZeroDelayVisitor visitor(diags);
        compilation.getRoot().visit(visitor);
    }
};

} // namespace uvmcheck::rules::performance

namespace uvmcheck::rules {
std::unique_ptr<Rule> createProceduralZeroDelay() {
    return std::make_unique<performance::ProceduralZeroDelay>();
}
}
