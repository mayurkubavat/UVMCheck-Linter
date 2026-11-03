#include "uvmcheck/rules/Rule.h"
#include "uvmcheck/rules/RuleRegistry.h"
#include "uvmcheck/model/SlangModel.h"

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

    // Reads only uvmcheck::model, not slang's AST.
    void run(slang::ast::Compilation&,
             const uvmcheck::uvm::UvmModel& uvmModel,
             core::DiagnosticEngine& diags) override {
        for (const model::Class& cls : uvmModel.facade().classes()) {
            if (cls.isParameterized) continue;
            for (const model::Constraint& c : cls.constraints) {
                if (c.name.ends_with("_c")) continue;
                diags.addViolation(
                    "UVM-STY-002",
                    "ConstraintNameSuffix",
                    "Constraint block '" + c.name + "' in class '" + cls.name +
                    "' does not follow the constraint naming convention. "
                    "Constraint block names must end with '_c' suffix (e.g. '" + c.name +
                    "_c') to clearly distinguish constraint sets from random variables in solver logs.",
                    core::Severity::Warning,
                    model::toSlang(c.loc)
                );
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

UVMCHECK_REGISTER_RULE(uvmcheck::rules::createConstraintNameSuffix)
