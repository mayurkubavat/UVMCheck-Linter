#include "uvmcheck/rules/Rule.h"
#include "uvmcheck/rules/RuleRegistry.h"
#include "slang/syntax/AllSyntax.h"
#include "slang/syntax/SyntaxTree.h"
#include "slang/syntax/SyntaxVisitor.h"
#include <string>
#include <algorithm>

namespace uvmcheck::rules::coverage {

class CoverOptionVisitor : public slang::syntax::SyntaxVisitor<CoverOptionVisitor> {
public:
    explicit CoverOptionVisitor(core::DiagnosticEngine& diags) : diags(diags) {}

    void handle(const slang::syntax::CoverageOptionSyntax& opt) {
        if (opt.expr) {
            std::string optText = std::string(opt.expr->toString());
            optText.erase(std::remove(optText.begin(), optText.end(), '\n'), optText.end());
            optText.erase(std::remove(optText.begin(), optText.end(), '\r'), optText.end());
            if (optText.find("weight") != std::string::npos &&
                opt.expr->kind == slang::syntax::SyntaxKind::AssignmentExpression) {
                const auto& assign = opt.expr->as<slang::syntax::BinaryExpressionSyntax>();
                if (assign.right) {
                    std::string rText = std::string(assign.right->toString());
                    rText.erase(std::remove_if(rText.begin(), rText.end(), ::isspace), rText.end());
                    if (rText == "0" || rText == "1'b0" || rText == "'0" || rText == "32'd0") {
                        std::string targetOpt = (optText.find("type_option") != std::string::npos) ?
                                                "type_option.weight" : "option.weight";
                        diags.addViolation(
                            "SV-COV-014",
                            "CoverpointWeightZero",
                            "Coverage option 'weight' is explicitly configured with value 0 ('" + targetOpt + " = " + rText + "'). "
                            "In IEEE 1800 § 19.5 coverage methodology, weight = 0 silently "
                            "excludes this covergroup or coverpoint from cumulative functional coverage metric computations, "
                            "creating a misleading impression of active coverage tracking without contributing to tape-out closure.",
                            core::Severity::Warning,
                            opt.expr->getFirstToken().location()
                        );
                    }
                }
            }
        }
        visitDefault(opt);
    }

private:
    core::DiagnosticEngine& diags;
};

class CoverpointWeightZero : public Rule {
public:
    std::string_view getId() const override { return "SV-COV-014"; }
    std::string_view getName() const override { return "CoverpointWeightZero"; }
    std::string_view getDescription() const override {
        return "Flags covergroups or coverpoints where 'option.weight' or 'type_option.weight' is set to 0, silently excluding it from coverage scores.";
    }
    std::string_view getCategory() const override { return "Coverage"; }
    core::Severity getDefaultSeverity() const override { return core::Severity::Warning; }

    void run(slang::ast::Compilation& compilation,
             const uvmcheck::uvm::UvmModel& /*uvmModel*/,
             core::DiagnosticEngine& diags) override {
        CoverOptionVisitor visitor(diags);
        for (const auto& tree : compilation.getSyntaxTrees()) {
            if (tree) {
                tree->root().visit(visitor);
            }
        }
    }
};

} // namespace uvmcheck::rules::coverage

namespace uvmcheck::rules {
std::unique_ptr<Rule> createCoverpointWeightZero() {
    return std::make_unique<coverage::CoverpointWeightZero>();
}
} // namespace uvmcheck::rules
