#include "uvmcheck/rules/Rule.h"
#include "uvmcheck/rules/RuleRegistry.h"
#include "slang/syntax/AllSyntax.h"
#include "slang/syntax/SyntaxTree.h"
#include "slang/syntax/SyntaxVisitor.h"

namespace uvmcheck::rules::coverage {

class UnboundedTransitionBinsVisitor : public slang::syntax::SyntaxVisitor<UnboundedTransitionBinsVisitor> {
public:
    explicit UnboundedTransitionBinsVisitor(core::DiagnosticEngine& diags) : diags(diags) {}

    void handle(const slang::syntax::CoverageBinsSyntax& node) {
        std::string prevBin = currentBinName;
        currentBinName = node.name ? std::string(node.name.valueText()) : "unnamed_bin";
        visitDefault(node);
        currentBinName = prevBin;
    }

    void handle(const slang::syntax::TransRepeatRangeSyntax& node) {
        std::string rangeStr = std::string(node.toString());
        if (rangeStr.find('$') != std::string::npos) {
            diags.addViolation(
                "SV-COV-005",
                "UnboundedTransitionBins",
                "Coverage transition bin '" + currentBinName + "' contains an unbounded repetition sequence '" +
                rangeStr + "'. Unbounded transition sequences ([*1:$], [->1:$]) require simulator coverage engines to track open threads indefinitely, triggering severe memory bloat and simulator slowness. Use a fixed upper bound.",
                core::Severity::Warning,
                node.sourceRange().start()
            );
        }
        visitDefault(node);
    }

private:
    core::DiagnosticEngine& diags;
    std::string currentBinName = "unnamed_bin";
};

class UnboundedTransitionBins : public Rule {
public:
    std::string_view getId() const override { return "SV-COV-005"; }
    std::string_view getName() const override { return "UnboundedTransitionBins"; }
    std::string_view getDescription() const override {
        return "Transition sequences in coverpoint bins must not specify unbounded repetitions (e.g., [*1:$], [->1:$]). "
               "Unbounded transition sequences cause exponential memory growth in simulator coverage databases.";
    }
    std::string_view getCategory() const override { return "Coverage"; }
    core::Severity getDefaultSeverity() const override { return core::Severity::Warning; }

    void run(slang::ast::Compilation& compilation,
             const uvmcheck::uvm::UvmModel& /*uvmModel*/,
             core::DiagnosticEngine& diags) override {
        UnboundedTransitionBinsVisitor visitor(diags);
        for (const auto& tree : compilation.getSyntaxTrees()) {
            if (tree) {
                tree->root().visit(visitor);
            }
        }
    }
};

} // namespace uvmcheck::rules::coverage

namespace uvmcheck::rules {
std::unique_ptr<Rule> createUnboundedTransitionBins() {
    return std::make_unique<coverage::UnboundedTransitionBins>();
}
}
