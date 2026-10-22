#include "uvmcheck/rules/Rule.h"
#include "uvmcheck/rules/RuleRegistry.h"
#include "slang/syntax/SyntaxVisitor.h"
#include "slang/syntax/AllSyntax.h"
#include "slang/syntax/SyntaxTree.h"

namespace uvmcheck::rules::sva {

class VacuousAntecedentVisitor : public engine::SyntaxVisitor<VacuousAntecedentVisitor> {
public:
    explicit VacuousAntecedentVisitor(core::DiagnosticEngine& diags) : diags(diags) {}

    void handle(const slang::syntax::BinaryPropertyExprSyntax& propExpr) {
        if (propExpr.kind == slang::syntax::SyntaxKind::ImplicationPropertyExpr) {
            std::string antStr = trim(std::string(propExpr.left->toString()));

            bool isVacuousFalse = false;
            if (antStr == "0" || antStr == "1'b0" || antStr == "'0" || antStr == "false" ||
                antStr == "1'sb0") {
                isVacuousFalse = true;
            } else if (antStr.find("&& !") != std::string::npos || antStr.find("&& ~") != std::string::npos) {
                // Check simple self-contradictions like (req && !req)
                isVacuousFalse = true;
            }

            if (isVacuousFalse) {
                diags.addViolation(
                    "SVA-VAC-001",
                    "SvaVacuousAntecedent",
                    "Vacuous assertion implication detected: antecedent '" + antStr + "' evaluates to constant false. "
                    "In SVA (IEEE 1800 § 16.12.22), a false antecedent makes the implication evaluate to true on every cycle, "
                    "causing the assertion to pass vacuously 100% of the time and masking unverified DUT logic.",
                    core::Severity::Error,
                    propExpr.left->sourceRange().start()
                );
            }
        }
        visitDefault(propExpr);
    }

private:
    static std::string trim(const std::string& str) {
        size_t first = str.find_first_not_of(" \t\r\n");
        if (first == std::string::npos) return "";
        size_t last = str.find_last_not_of(" \t\r\n");
        return str.substr(first, last - first + 1);
    }

    core::DiagnosticEngine& diags;
};

class SvaVacuousAntecedent : public Rule {
public:
    std::string_view getId() const override { return "SVA-VAC-001"; }
    std::string_view getName() const override { return "SvaVacuousAntecedent"; }
    std::string_view getDescription() const override {
        return "Detects constant-false antecedents in assertion implications (0 |-> ...). False antecedents cause "
               "vacuous passes, giving a false 100% pass metric while never verifying DUT behavior.";
    }
    std::string_view getCategory() const override { return "SVA"; }
    core::Severity getDefaultSeverity() const override { return core::Severity::Error; }

    engine::RuleTask check(engine::RuleContext& ctx) override {
        auto& diags = ctx.diags();
        VacuousAntecedentVisitor visitor(diags);
        co_await ctx.visitSyntax(visitor);
    }
};

} // namespace uvmcheck::rules::sva

namespace uvmcheck::rules {
std::unique_ptr<Rule> createSvaVacuousAntecedent() {
    return std::make_unique<sva::SvaVacuousAntecedent>();
}
}
