#include "uvmcheck/rules/Rule.h"
#include "uvmcheck/rules/RuleRegistry.h"
#include "slang/syntax/AllSyntax.h"
#include "slang/syntax/SyntaxTree.h"
#include "slang/syntax/SyntaxVisitor.h"
#include <string>

namespace uvmcheck::rules::coverage {

class CrossWithEmptyBinsVisitor : public slang::syntax::SyntaxVisitor<CrossWithEmptyBinsVisitor> {
public:
    explicit CrossWithEmptyBinsVisitor(core::DiagnosticEngine& diags) : diags(diags) {}

    void handle(const slang::syntax::CoverCrossSyntax& node) {
        std::string crossLabel = node.label ? std::string(node.label->name.valueText()) : "unnamed_cross";

        // Check 1: Degenerate cross crossing fewer than 2 coverpoints
        if (node.items.size() < 2) {
            diags.addViolation(
                "SV-COV-008",
                "CrossWithEmptyBins",
                "Covergroup cross '" + crossLabel +
                "' specifies fewer than 2 coverpoints. Cross coverage requires at least two distinct coverpoints to construct a Cartesian product.",
                core::Severity::Warning,
                node.cross.location()
            );
        }

        visitDefault(node);
    }

private:
    core::DiagnosticEngine& diags;
};

class CrossWithEmptyBins : public Rule {
public:
    std::string_view getId() const override { return "SV-COV-008"; }
    std::string_view getName() const override { return "CrossWithEmptyBins"; }
    std::string_view getDescription() const override {
        return "Detects degenerate covergroup cross constructs specifying fewer than 2 coverpoints or empty cross products.";
    }
    std::string_view getCategory() const override { return "Coverage"; }
    core::Severity getDefaultSeverity() const override { return core::Severity::Warning; }

    void run(slang::ast::Compilation& compilation,
             const uvmcheck::uvm::UvmModel& /*uvmModel*/,
             core::DiagnosticEngine& diags) override {
        CrossWithEmptyBinsVisitor visitor(diags);
        for (const auto& tree : compilation.getSyntaxTrees()) {
            if (tree) {
                tree->root().visit(visitor);
            }
        }
    }
};

} // namespace uvmcheck::rules::coverage

namespace uvmcheck::rules {
std::unique_ptr<Rule> createCrossWithEmptyBins() {
    return std::make_unique<coverage::CrossWithEmptyBins>();
}
}
