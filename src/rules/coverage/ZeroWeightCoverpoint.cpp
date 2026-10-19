#include "uvmcheck/rules/Rule.h"
#include "uvmcheck/rules/RuleRegistry.h"
#include "slang/syntax/AllSyntax.h"
#include "slang/syntax/SyntaxTree.h"
#include "slang/syntax/SyntaxVisitor.h"

namespace uvmcheck::rules::coverage {

class ZeroWeightCoverpointVisitor : public slang::syntax::SyntaxVisitor<ZeroWeightCoverpointVisitor> {
public:
    explicit ZeroWeightCoverpointVisitor(core::DiagnosticEngine& diags) : diags(diags) {}

    void handle(const slang::syntax::CovergroupDeclarationSyntax& node) {
        std::string prevCg = currentCovergroup;
        currentCovergroup = node.name ? std::string(node.name.valueText()) : "unnamed_cg";
        visitDefault(node);
        currentCovergroup = prevCg;
    }

    void handle(const slang::syntax::CoverpointSyntax& node) {
        std::string prevCp = currentItem;
        currentItem = node.label ? std::string(node.label->name.valueText()) : "unnamed_cp";
        visitDefault(node);
        currentItem = prevCp;
    }

    void handle(const slang::syntax::CoverCrossSyntax& node) {
        std::string prevCr = currentItem;
        currentItem = node.label ? std::string(node.label->name.valueText()) : "unnamed_cross";
        visitDefault(node);
        currentItem = prevCr;
    }

    void handle(const slang::syntax::CoverageOptionSyntax& node) {
        if (node.expr) {
            std::string exprText = std::string(node.expr->toString());
            if (exprText.find("weight") != std::string::npos) {
                if (const auto* binExpr = node.expr->as_if<slang::syntax::BinaryExpressionSyntax>()) {
                    if (binExpr->kind == slang::syntax::SyntaxKind::AssignmentExpression) {
                        std::string rhs = std::string(binExpr->right->toString());
                        // Trim whitespace
                        rhs.erase(0, rhs.find_first_not_of(" \t\n\r"));
                        if (size_t end = rhs.find_last_not_of(" \t\n\r"); end != std::string::npos) {
                            rhs.erase(end + 1);
                        }
                        if (rhs == "0" || rhs == "'0" || rhs == "32'd0" || rhs == "32'b0") {
                            std::string target = currentItem.empty() ? currentCovergroup : currentItem;
                            diags.addViolation(
                                "SV-COV-002",
                                "ZeroWeightCoverpoint",
                                "Coverage construct '" + target + "' specifies zero weight ('" + exprText + "'). "
                                "Setting option.weight or type_option.weight to 0 causes this construct to contribute 0% to regression coverage calculation, masking untested logic.",
                                core::Severity::Warning,
                                node.sourceRange().start()
                            );
                        }
                    }
                }
            }
        }
        visitDefault(node);
    }

private:
    core::DiagnosticEngine& diags;
    std::string currentCovergroup;
    std::string currentItem;
};

class ZeroWeightCoverpoint : public Rule {
public:
    std::string_view getId() const override { return "SV-COV-002"; }
    std::string_view getName() const override { return "ZeroWeightCoverpoint"; }
    std::string_view getDescription() const override {
        return "Flags covergroups, coverpoints, or crosses configured with option.weight or type_option.weight set to 0. "
               "Zero-weight coverage constructs collect samples but contribute 0% to regression coverage closure.";
    }
    std::string_view getCategory() const override { return "Coverage"; }
    core::Severity getDefaultSeverity() const override { return core::Severity::Warning; }

    void run(slang::ast::Compilation& compilation,
             const uvmcheck::uvm::UvmModel& /*uvmModel*/,
             core::DiagnosticEngine& diags) override {
        ZeroWeightCoverpointVisitor visitor(diags);
        for (const auto& tree : compilation.getSyntaxTrees()) {
            if (tree) {
                tree->root().visit(visitor);
            }
        }
    }
};

} // namespace uvmcheck::rules::coverage

namespace uvmcheck::rules {
std::unique_ptr<Rule> createZeroWeightCoverpoint() {
    return std::make_unique<coverage::ZeroWeightCoverpoint>();
}
}
