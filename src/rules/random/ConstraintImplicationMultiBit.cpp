#include "uvmcheck/rules/Rule.h"
#include "uvmcheck/rules/RuleRegistry.h"
#include "slang/ast/Constraints.h"
#include "slang/ast/symbols/ClassSymbols.h"
#include "slang/ast/types/AllTypes.h"

namespace uvmcheck::rules::random {

static void inspectConstraint(const slang::ast::Constraint& c, core::DiagnosticEngine& diags) {
    switch (c.kind) {
        case slang::ast::ConstraintKind::Implication: {
            const auto& imp = c.as<slang::ast::ImplicationConstraint>();
            if (imp.predicate.type && imp.predicate.type->isIntegral() && imp.predicate.type->getBitWidth() > 1) {
                diags.addViolation(
                    "SV-RND-019",
                    "ConstraintImplicationMultiBit",
                    "Constraint implication operator '->' has a multi-bit condition expression of width " +
                    std::to_string(imp.predicate.type->getBitWidth()) + " bits without an explicit boolean comparison. "
                    "In SystemVerilog constraint solving (IEEE 1800 § 18.5.6), multi-bit implication is often confused with bitwise masking; "
                    "use an explicit comparison (e.g. 'expr != 0 -> ...') to make constraint intent unambiguous.",
                    core::Severity::Warning,
                    imp.predicate.sourceRange.start()
                );
            }
            inspectConstraint(imp.body, diags);
            break;
        }
        case slang::ast::ConstraintKind::List: {
            const auto& list = c.as<slang::ast::ConstraintList>();
            for (const auto* item : list.list) {
                if (item) inspectConstraint(*item, diags);
            }
            break;
        }
        case slang::ast::ConstraintKind::Conditional: {
            const auto& cond = c.as<slang::ast::ConditionalConstraint>();
            inspectConstraint(cond.ifBody, diags);
            if (cond.elseBody) inspectConstraint(*cond.elseBody, diags);
            break;
        }
        case slang::ast::ConstraintKind::Foreach: {
            const auto& fe = c.as<slang::ast::ForeachConstraint>();
            inspectConstraint(fe.body, diags);
            break;
        }
        default:
            break;
    }
}

class ConstraintImplicationMultiBit : public Rule {
public:
    std::string_view getId() const override { return "SV-RND-019"; }
    std::string_view getName() const override { return "ConstraintImplicationMultiBit"; }
    std::string_view getDescription() const override {
        return "Flags constraint implication expressions 'A -> B' where condition 'A' is a multi-bit vector without an explicit boolean comparison.";
    }
    std::string_view getCategory() const override { return "Randomization"; }
    core::Severity getDefaultSeverity() const override { return core::Severity::Warning; }

    void run(slang::ast::Compilation& /*compilation*/,
             const uvmcheck::uvm::UvmModel& uvmModel,
             core::DiagnosticEngine& diags) override {
        for (const auto* cls : uvmModel.getAllClasses()) {
            if (!cls) continue;
            for (const auto& member : cls->members()) {
                if (member.kind == slang::ast::SymbolKind::ConstraintBlock) {
                    const auto& cb = member.as<slang::ast::ConstraintBlockSymbol>();
                    inspectConstraint(cb.getConstraints(), diags);
                }
            }
        }
    }
};

} // namespace uvmcheck::rules::random

namespace uvmcheck::rules {
std::unique_ptr<Rule> createConstraintImplicationMultiBit() {
    return std::make_unique<random::ConstraintImplicationMultiBit>();
}
} // namespace uvmcheck::rules
