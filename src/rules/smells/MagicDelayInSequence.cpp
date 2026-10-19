#include "uvmcheck/rules/Rule.h"
#include "uvmcheck/rules/RuleRegistry.h"
#include "slang/ast/ASTVisitor.h"
#include "slang/ast/TimingControl.h"
#include "slang/ast/symbols/ClassSymbols.h"
#include "slang/ast/statements/MiscStatements.h"
#include "slang/ast/expressions/MiscExpressions.h"
#include "slang/syntax/AllSyntax.h"

namespace uvmcheck::rules::smells {

class SequenceDelayVisitor : public slang::ast::ASTVisitor<SequenceDelayVisitor, slang::ast::VisitFlags::AllGood> {
public:
    explicit SequenceDelayVisitor(const uvmcheck::uvm::UvmModel& uvmModel, core::DiagnosticEngine& diags)
        : uvmModel(uvmModel), diags(diags) {}

    void handle(const slang::ast::ClassType& cls) {
        bool wasInSequence = inSequence;
        if (uvmModel.isUvmSequence(cls)) {
            inSequence = true;
        }
        visitDefault(cls);
        inSequence = wasInSequence;
    }

    void handle(const slang::ast::TimedStatement& stmt) {
        if (inSequence && stmt.timing.kind == slang::ast::TimingControlKind::Delay) {
            const auto& delay = stmt.timing.as<slang::ast::DelayControl>();
            bool isZero = false;
            if (delay.expr.kind == slang::ast::ExpressionKind::IntegerLiteral) {
                if (delay.expr.as<slang::ast::IntegerLiteral>().getValue() == 0) {
                    isZero = true;
                }
            }

            if (!isZero) {
                std::string delayStr = delay.expr.syntax ? std::string(delay.expr.syntax->toString()) : "delay";
                diags.addViolation(
                    "SV-SML-002",
                    "MagicDelayInSequence",
                    "Hardcoded physical time delay '#" + delayStr + "' inside uvm_sequence. "
                    "Sequences must be clock-frequency agnostic to ensure portability across emulation, FPGA prototyping, and ASIC frequencies. "
                    "Synchronize using clocking blocks ('repeat(N) @(vif.cb)') or protocol handshakes ('wait(vif.ready)') instead.",
                    core::Severity::Warning,
                    stmt.sourceRange.start()
                );
            }
        }
        visitDefault(stmt);
    }

private:
    bool inSequence = false;
    const uvmcheck::uvm::UvmModel& uvmModel;
    core::DiagnosticEngine& diags;
};

class MagicDelayInSequence : public Rule {
public:
    std::string_view getId() const override { return "SV-SML-002"; }
    std::string_view getName() const override { return "MagicDelayInSequence"; }
    std::string_view getDescription() const override {
        return "Disallow hardcoded physical delays (#delay) inside uvm_sequence. Sequences must be clocking-block and frequency agnostic.";
    }
    std::string_view getCategory() const override { return "Smell"; }
    core::Severity getDefaultSeverity() const override { return core::Severity::Warning; }

    void run(slang::ast::Compilation& compilation,
             const uvmcheck::uvm::UvmModel& uvmModel,
             core::DiagnosticEngine& diags) override {
        SequenceDelayVisitor visitor(uvmModel, diags);
        compilation.getRoot().visit(visitor);
    }
};

} // namespace uvmcheck::rules::smells

namespace uvmcheck::rules {
std::unique_ptr<Rule> createMagicDelayInSequence() {
    return std::make_unique<smells::MagicDelayInSequence>();
}
}
