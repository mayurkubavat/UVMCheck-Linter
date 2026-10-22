#include "uvmcheck/rules/Rule.h"
#include "uvmcheck/rules/RuleRegistry.h"
#include "slang/syntax/AllSyntax.h"
#include "slang/syntax/SyntaxTree.h"
#include "slang/syntax/SyntaxVisitor.h"
#include <string>

namespace uvmcheck::rules::uvm {

class StartOnNullVisitor : public engine::SyntaxVisitor<StartOnNullVisitor> {
public:
    explicit StartOnNullVisitor(core::DiagnosticEngine& diags) : diags(diags) {}

    void handle(const slang::syntax::InvocationExpressionSyntax& inv) {
        if (inv.left) {
            std::string methodName = std::string(inv.left->getLastToken().valueText());
            if (methodName == "start" && inv.arguments && !inv.arguments->parameters.empty()) {
                const auto* firstArg = inv.arguments->parameters[0];
                if (firstArg->kind == slang::syntax::SyntaxKind::OrderedArgument) {
                    const auto& ordArg = firstArg->as<slang::syntax::OrderedArgumentSyntax>();
                    std::string argStr = std::string(ordArg.expr->toString());
                    argStr.erase(0, argStr.find_first_not_of(" \t\n\r"));
                    if (size_t end = argStr.find_last_not_of(" \t\n\r"); end != std::string::npos) {
                        argStr.erase(end + 1);
                    }

                    if (argStr == "null") {
                        std::string seqName = "seq";
                        std::string leftStr = std::string(inv.left->toString());
                        size_t dotPos = leftStr.rfind(".start");
                        if (dotPos != std::string::npos) {
                            seqName = leftStr.substr(0, dotPos);
                            if (size_t lastNl = seqName.rfind('\n'); lastNl != std::string::npos) {
                                seqName = seqName.substr(lastNl + 1);
                            }
                            seqName.erase(0, seqName.find_first_not_of(" \t\r"));
                            if (size_t end = seqName.find_last_not_of(" \t\r"); end != std::string::npos) {
                                seqName.erase(end + 1);
                            }
                        }

                        diags.addViolation(
                            "UVM-SEQ-012",
                            "SequenceStartOnNullSequencer",
                            "Sequence invocation '" + seqName + ".start(null)' starts sequence with null sequencer handle. "
                            "In UVM sequence architecture (IEEE 1800.2 § 5.8), "
                            "starting a sequence on 'null' leaves 'm_sequencer' and 'p_sequencer' unattached, causing immediate "
                            "fatal null-pointer crashes when the sequence attempts to send transactions (start_item/finish_item) "
                            "or query sequencer configuration. Pass a valid sequencer reference (e.g. 'm_sequencer' or 'p_sequencer').",
                            core::Severity::Warning,
                            ordArg.expr->getFirstToken().location()
                        );
                    }
                }
            }
        }
        visitDefault(inv);
    }

private:
    core::DiagnosticEngine& diags;
};

class SequenceStartOnNullSequencer : public Rule {
public:
    std::string_view getId() const override { return "UVM-SEQ-012"; }
    std::string_view getName() const override { return "SequenceStartOnNullSequencer"; }
    std::string_view getDescription() const override {
        return "Flags sequence start(null) calls that start sequences without an attached sequencer handle.";
    }
    std::string_view getCategory() const override { return "UVM"; }
    core::Severity getDefaultSeverity() const override { return core::Severity::Warning; }

    engine::RuleTask check(engine::RuleContext& ctx) override {
        auto& diags = ctx.diags();
        StartOnNullVisitor visitor(diags);
        co_await ctx.visitSyntax(visitor);
    }
};

} // namespace uvmcheck::rules::uvm

namespace uvmcheck::rules {
std::unique_ptr<Rule> createSequenceStartOnNullSequencer() {
    return std::make_unique<uvm::SequenceStartOnNullSequencer>();
}
} // namespace uvmcheck::rules
