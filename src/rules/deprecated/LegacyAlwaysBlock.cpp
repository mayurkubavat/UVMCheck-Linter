#include "uvmcheck/rules/Rule.h"
#include "uvmcheck/rules/RuleRegistry.h"
#include "slang/syntax/SyntaxVisitor.h"
#include "slang/syntax/SyntaxTree.h"
#include "slang/syntax/AllSyntax.h"

namespace uvmcheck::rules::deprecated {

class LegacyAlwaysVisitor : public slang::syntax::SyntaxVisitor<LegacyAlwaysVisitor> {
public:
    explicit LegacyAlwaysVisitor(core::DiagnosticEngine& diags) : diags(diags) {}

    void handle(const slang::syntax::ProceduralBlockSyntax& block) {
        if (block.kind == slang::syntax::SyntaxKind::AlwaysBlock) {
            diags.addViolation(
                "SV-DEP-003",
                "LegacyAlwaysBlock",
                "Legacy untyped 'always' block is superseded in IEEE 1800 § 9.2.2. "
                "Untyped always blocks permit unintended latch inference and incomplete sensitivity lists. "
                "Use intent-specific procedural blocks: 'always_comb' for combinational logic, "
                "'always_ff @(posedge clk ...)' for sequential logic, or 'always_latch' for transparent latches.",
                core::Severity::Warning,
                block.keyword.location()
            );
        }
        visitDefault(block);
    }

private:
    core::DiagnosticEngine& diags;
};

class LegacyAlwaysBlock : public Rule {
public:
    std::string_view getId() const override { return "SV-DEP-003"; }
    std::string_view getName() const override { return "LegacyAlwaysBlock"; }
    std::string_view getDescription() const override {
        return "Disallow untyped 'always' blocks. Enforces IEEE 1800 intent-specific blocks: "
               "always_comb, always_ff, or always_latch.";
    }
    std::string_view getCategory() const override { return "Deprecated"; }
    core::Severity getDefaultSeverity() const override { return core::Severity::Warning; }

    void run(slang::ast::Compilation& compilation,
             const uvmcheck::uvm::UvmModel&,
             core::DiagnosticEngine& diags) override {
        LegacyAlwaysVisitor visitor(diags);
        for (const auto& tree : compilation.getSyntaxTrees()) {
            tree->root().visit(visitor);
        }
    }
};

} // namespace uvmcheck::rules::deprecated

namespace uvmcheck::rules {
std::unique_ptr<Rule> createLegacyAlwaysBlock() {
    return std::make_unique<deprecated::LegacyAlwaysBlock>();
}
}
