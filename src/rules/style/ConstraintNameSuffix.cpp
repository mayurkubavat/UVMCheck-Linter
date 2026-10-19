#include "uvmcheck/rules/Rule.h"
#include "uvmcheck/rules/RuleRegistry.h"
#include "slang/ast/symbols/ClassSymbols.h"

namespace uvmcheck::rules::style {

class ConstraintNameSuffix : public Rule {
public:
    std::string_view getId() const override { return "UVM-STY-002"; }
    std::string_view getName() const override { return "ConstraintNameSuffix"; }
    std::string_view getDescription() const override {
        return "Enforces a naming standard requiring all constraint blocks to end with '_c' suffix.";
    }
    std::string_view getCategory() const override { return "Style"; }
    core::Severity getDefaultSeverity() const override { return core::Severity::Warning; }

    void run(slang::ast::Compilation&,
             const uvmcheck::uvm::UvmModel& uvmModel,
             core::DiagnosticEngine& diags) override {
        for (const auto* cls : uvmModel.getAllClasses()) {
            if (!cls) continue;

            for (const auto& member : cls->members()) {
                if (member.kind == slang::ast::SymbolKind::ConstraintBlock) {
                    const auto& c = member.as<slang::ast::ConstraintBlockSymbol>();
                    if (!c.name.ends_with("_c")) {
                        diags.addViolation(
                            "UVM-STY-002",
                            "ConstraintNameSuffix",
                            "Constraint block '" + std::string(c.name) + "' in class '" + std::string(cls->name) +
                            "' does not follow the constraint naming convention. "
                            "Constraint block names must end with '_c' suffix (e.g. '" + std::string(c.name) +
                            "_c') to clearly distinguish constraint sets from random variables in solver logs.",
                            core::Severity::Warning,
                            c.location
                        );
                    }
                }
            }
        }
    }
};

} // namespace uvmcheck::rules::style

namespace uvmcheck::rules {
std::unique_ptr<Rule> createConstraintNameSuffix() {
    return std::make_unique<style::ConstraintNameSuffix>();
}
}
