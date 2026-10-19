#include "uvmcheck/rules/Rule.h"
#include "uvmcheck/rules/RuleRegistry.h"
#include "slang/syntax/AllSyntax.h"
#include "slang/syntax/SyntaxTree.h"
#include "slang/syntax/SyntaxVisitor.h"

namespace uvmcheck::rules::coverage {

class AtLeastZeroOrNegativeVisitor : public slang::syntax::SyntaxVisitor<AtLeastZeroOrNegativeVisitor> {
public:
    explicit AtLeastZeroOrNegativeVisitor(core::DiagnosticEngine& diags) : diags(diags) {}

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
            if (exprText.find("at_least") != std::string::npos) {
                if (const auto* binExpr = node.expr->as_if<slang::syntax::BinaryExpressionSyntax>()) {
                    if (binExpr->kind == slang::syntax::SyntaxKind::AssignmentExpression) {
                        std::string rhs = std::string(binExpr->right->toString());
                        rhs.erase(0, rhs.find_first_not_of(" \t\n\r"));
                        if (size_t end = rhs.find_last_not_of(" \t\n\r"); end != std::string::npos) {
                            rhs.erase(end + 1);
                        }

                        bool isZeroOrNegative = false;
                        if (rhs == "0" || rhs == "'0" || rhs == "32'd0" || rhs == "32'b0" ||
                            (!rhs.empty() && rhs[0] == '-')) {
                            isZeroOrNegative = true;
                        }

                        if (isZeroOrNegative) {
                            std::string target = currentItem.empty() ? currentCovergroup : currentItem;
                            std::vector<core::FixItHint> fixIts;
                            fixIts.push_back({
                                binExpr->right->sourceRange(),
                                "1",
                                "Set at_least to 1"
                            });

                            diags.addViolation(
                                "SV-COV-011",
                                "AtLeastZeroOrNegative",
                                "Coverage construct '" + target + "' specifies non-positive 'at_least' threshold ('" +
                                rhs + "'). In IEEE 1800 § 19.5, 'option.at_least' and 'type_option.at_least' define "
                                "the minimum number of hit occurrences required for a bin to be considered covered and must be an integer >= 1. "
                                "Setting at_least <= 0 causes bins to be trivially covered without verification or invalidates coverage metrics.",
                                core::Severity::Warning,
                                node.sourceRange().start(),
                                std::move(fixIts)
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

class AtLeastZeroOrNegative : public Rule {
public:
    std::string_view getId() const override { return "SV-COV-011"; }
    std::string_view getName() const override { return "AtLeastZeroOrNegative"; }
    std::string_view getDescription() const override {
        return "Flags option.at_least or type_option.at_least set to 0 or negative values in covergroups/coverpoints. "
               "IEEE 1800 § 19.5 requires at_least to be a positive integer >= 1.";
    }
    std::string_view getCategory() const override { return "Coverage"; }
    core::Severity getDefaultSeverity() const override { return core::Severity::Warning; }

    void run(slang::ast::Compilation& compilation,
             const uvmcheck::uvm::UvmModel& /*uvmModel*/,
             core::DiagnosticEngine& diags) override {
        AtLeastZeroOrNegativeVisitor visitor(diags);
        for (const auto& tree : compilation.getSyntaxTrees()) {
            if (tree) {
                tree->root().visit(visitor);
            }
        }
    }
};

} // namespace uvmcheck::rules::coverage

namespace uvmcheck::rules {
std::unique_ptr<Rule> createAtLeastZeroOrNegative() {
    return std::make_unique<coverage::AtLeastZeroOrNegative>();
}
}
