#include "uvmcheck/rules/Rule.h"
#include "uvmcheck/rules/RuleRegistry.h"
#include "slang/syntax/AllSyntax.h"
#include "slang/syntax/SyntaxTree.h"
#include "slang/syntax/SyntaxVisitor.h"
#include <string>

namespace uvmcheck::rules::uvm {

class SequencePriorityVisitor : public slang::syntax::SyntaxVisitor<SequencePriorityVisitor> {
public:
    explicit SequencePriorityVisitor(core::DiagnosticEngine& diags) : diags(diags) {}

    void handle(const slang::syntax::InvocationExpressionSyntax& inv) {
        if (inv.left) {
            std::string methodName = std::string(inv.left->getLastToken().valueText());
            if (methodName == "start" && inv.arguments && inv.arguments->parameters.size() >= 3) {
                const auto* prioArg = inv.arguments->parameters[2];
                if (prioArg->kind == slang::syntax::SyntaxKind::OrderedArgument) {
                    const auto& ordArg = prioArg->as<slang::syntax::OrderedArgumentSyntax>();
                    std::string prioStr = std::string(ordArg.expr->toString());
                    prioStr.erase(0, prioStr.find_first_not_of(" \t\n\r"));
                    if (size_t end = prioStr.find_last_not_of(" \t\n\r"); end != std::string::npos) {
                        prioStr.erase(end + 1);
                    }

                    // In UVM, default priority is -1 (inherit default). Values < -1 (e.g. -2, -10, -100) are invalid.
                    bool isInvalidNegative = false;
                    if (prioStr.starts_with("-") && prioStr != "-1") {
                        isInvalidNegative = true;
                    }

                    if (isInvalidNegative) {
                        diags.addViolation(
                            "UVM-SEQ-007",
                            "SequencePriorityRange",
                            "Invalid sequence priority value '" + prioStr +
                            "' passed to 'seq.start()'. In UVM, sequence priorities must be non-negative integers >= 0 (or -1 to inherit the sequencer default priority 100). Arbitrary negative values corrupt arbitration hierarchies.",
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

class SequencePriorityRange : public Rule {
public:
    std::string_view getId() const override { return "UVM-SEQ-007"; }
    std::string_view getName() const override { return "SequencePriorityRange"; }
    std::string_view getDescription() const override {
        return "Flags sequence start() calls passing invalid negative priority values (< -1) which corrupt sequencer arbitration.";
    }
    std::string_view getCategory() const override { return "UVM"; }
    core::Severity getDefaultSeverity() const override { return core::Severity::Warning; }

    void run(slang::ast::Compilation& compilation,
             const uvmcheck::uvm::UvmModel& /*uvmModel*/,
             core::DiagnosticEngine& diags) override {
        SequencePriorityVisitor visitor(diags);
        for (const auto& tree : compilation.getSyntaxTrees()) {
            if (tree) {
                tree->root().visit(visitor);
            }
        }
    }
};

} // namespace uvmcheck::rules::uvm

namespace uvmcheck::rules {
std::unique_ptr<Rule> createSequencePriorityRange() {
    return std::make_unique<uvm::SequencePriorityRange>();
}
}
