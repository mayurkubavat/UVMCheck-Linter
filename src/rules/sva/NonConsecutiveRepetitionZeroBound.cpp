#include "uvmcheck/rules/Rule.h"
#include "uvmcheck/rules/RuleRegistry.h"
#include "slang/syntax/AllSyntax.h"
#include "slang/syntax/SyntaxTree.h"
#include "slang/syntax/SyntaxVisitor.h"
#include <string>
#include <algorithm>

namespace uvmcheck::rules::sva {

class RepetitionZeroVisitor : public slang::syntax::SyntaxVisitor<RepetitionZeroVisitor> {
public:
    explicit RepetitionZeroVisitor(core::DiagnosticEngine& diags) : diags(diags) {}

    void handle(const slang::syntax::SequenceRepetitionSyntax& rep) {
        std::string opStr = std::string(rep.op.valueText());
        if (opStr == "->" || opStr == "=") {
            if (rep.selector) {
                std::string selStr = std::string(rep.selector->toString());
                selStr.erase(std::remove_if(selStr.begin(), selStr.end(), ::isspace), selStr.end());

                bool isZero = (selStr == "0" || selStr == "1'b0" || selStr == "32'd0" ||
                               selStr.starts_with("0:") || selStr.starts_with("1'b0:"));

                if (isZero) {
                    diags.addViolation(
                        "SV-SVA-004",
                        "NonConsecutiveRepetitionZeroBound",
                        "Non-consecutive repetition operator '[" + opStr + " " + selStr +
                        "]' specifies a zero-repetition bound. Under IEEE 1800 § 16.9.2 assertion semantics, "
                        "non-consecutive repetitions ('[= ...]' and '[-> ...]') must specify a positive minimum repetition bound (>= 1). "
                        "Repetition bounds of 0 are vacuous, illegal under standard LRM rules, and cause assertion solver aborts.",
                        core::Severity::Error,
                        rep.op.location()
                    );
                }
            }
        }
        visitDefault(rep);
    }

private:
    core::DiagnosticEngine& diags;
};

class NonConsecutiveRepetitionZeroBound : public Rule {
public:
    std::string_view getId() const override { return "SV-SVA-004"; }
    std::string_view getName() const override { return "NonConsecutiveRepetitionZeroBound"; }
    std::string_view getDescription() const override {
        return "Non-consecutive repetition operators ([= ...] and [-> ...]) must specify a minimum bound >= 1.";
    }
    std::string_view getCategory() const override { return "SVA"; }
    core::Severity getDefaultSeverity() const override { return core::Severity::Error; }

    void run(slang::ast::Compilation& compilation,
             const uvmcheck::uvm::UvmModel& /*uvmModel*/,
             core::DiagnosticEngine& diags) override {
        RepetitionZeroVisitor visitor(diags);
        for (const auto& tree : compilation.getSyntaxTrees()) {
            if (tree) {
                tree->root().visit(visitor);
            }
        }
    }
};

} // namespace uvmcheck::rules::sva

namespace uvmcheck::rules {
std::unique_ptr<Rule> createNonConsecutiveRepetitionZeroBound() {
    return std::make_unique<sva::NonConsecutiveRepetitionZeroBound>();
}
} // namespace uvmcheck::rules
