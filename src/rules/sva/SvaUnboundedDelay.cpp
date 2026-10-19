#include "uvmcheck/rules/Rule.h"
#include "uvmcheck/rules/RuleRegistry.h"
#include "slang/syntax/SyntaxVisitor.h"
#include "slang/syntax/AllSyntax.h"
#include "slang/syntax/SyntaxTree.h"

namespace uvmcheck::rules::sva {

class UnboundedDelayVisitor : public slang::syntax::SyntaxVisitor<UnboundedDelayVisitor> {
public:
    explicit UnboundedDelayVisitor(core::DiagnosticEngine& diags) : diags(diags) {}

    void handle(const slang::syntax::DelayedSequenceElementSyntax& elem) {
        if (elem.range) {
            std::string rangeStr = std::string(elem.range->toString());
            if (rangeStr.find('$') != std::string::npos) {
                diags.addViolation(
                    "SVA-CYC-001",
                    "SvaUnboundedDelay",
                    "Unbounded delay cycle range '##[" + rangeStr + "]' in assertion sequence. "
                    "In simulation, unbounded delays keep evaluation threads alive indefinitely in memory waiting for unreachable events. "
                    "In formal property verification (FPV), unbounded delays cause infinite induction unrolling and proof timeouts. "
                    "Specify a bounded maximum delay (e.g. '##[1:MAX_CYCLES]') or use the 's_eventually' liveness operator.",
                    core::Severity::Warning,
                    elem.sourceRange().start()
                );
            }
        }
        visitDefault(elem);
    }

    void handle(const slang::syntax::DelaySyntax& delay) {
        if (delay.delayValue) {
            std::string delayStr = std::string(delay.delayValue->toString());
            if (delayStr.find('$') != std::string::npos) {
                diags.addViolation(
                    "SVA-CYC-001",
                    "SvaUnboundedDelay",
                    "Unbounded delay cycle range '##" + delayStr + "' in assertion sequence. "
                    "In simulation, unbounded delays keep evaluation threads alive indefinitely in memory waiting for unreachable events. "
                    "In formal property verification (FPV), unbounded delays cause infinite induction unrolling and proof timeouts. "
                    "Specify a bounded maximum delay (e.g. '##[1:MAX_CYCLES]') or use the 's_eventually' liveness operator.",
                    core::Severity::Warning,
                    delay.sourceRange().start()
                );
            }
        }
        visitDefault(delay);
    }

private:
    core::DiagnosticEngine& diags;
};

class SvaUnboundedDelay : public Rule {
public:
    std::string_view getId() const override { return "SVA-CYC-001"; }
    std::string_view getName() const override { return "SvaUnboundedDelay"; }
    std::string_view getDescription() const override {
        return "Disallow unbounded cycle delays (##[1:$]) in SVA sequences. Unbounded delays cause simulator "
               "memory thread leaks and formal verification state-space explosions.";
    }
    std::string_view getCategory() const override { return "SVA"; }
    core::Severity getDefaultSeverity() const override { return core::Severity::Warning; }

    void run(slang::ast::Compilation& compilation,
             const uvmcheck::uvm::UvmModel& /*uvmModel*/,
             core::DiagnosticEngine& diags) override {
        UnboundedDelayVisitor visitor(diags);
        for (const auto& syntaxTree : compilation.getSyntaxTrees()) {
            if (syntaxTree) {
                syntaxTree->root().visit(visitor);
            }
        }
    }
};

} // namespace uvmcheck::rules::sva

namespace uvmcheck::rules {
std::unique_ptr<Rule> createSvaUnboundedDelay() {
    return std::make_unique<sva::SvaUnboundedDelay>();
}
}
