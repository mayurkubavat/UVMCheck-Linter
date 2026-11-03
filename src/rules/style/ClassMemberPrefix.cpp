#include "uvmcheck/rules/Rule.h"
#include "uvmcheck/rules/RuleRegistry.h"
#include "uvmcheck/model/SlangModel.h"

namespace uvmcheck::rules::style {

class ClassMemberPrefix : public Rule {
public:
    std::string_view getId() const override { return "SV-STY-001"; }
    std::string_view getName() const override { return "ClassMemberPrefix"; }
    std::string_view getDescription() const override {
        return "Enforces that member properties of uvm_component classes begin with 'm_' prefix "
               "to distinguish local member state from arguments and parameters.";
    }
    std::string_view getCategory() const override { return "Style"; }
    core::Severity getDefaultSeverity() const override { return core::Severity::Info; }

    // Reads only uvmcheck::model, not slang's AST.
    void run(slang::ast::Compilation& /*compilation*/,
             const uvmcheck::uvm::UvmModel& uvmModel,
             core::DiagnosticEngine& diags) override {
        for (const model::Class& cls : uvmModel.facade().classes()) {
            if (cls.isParameterized || !model::isComponent(cls.uvm)) continue;

            // Skip internal UVM base classes
            if (cls.name.starts_with("uvm_")) continue;

            for (const model::Property& prop : cls.properties) {
                if (prop.isConst || prop.isStatic) continue;

                // Built-in / macro injected properties
                if (prop.name == "type_name" || prop.name == "type_id") continue;

                if (!prop.name.starts_with("m_")) {
                    diags.addViolation(
                        "SV-STY-001",
                        "ClassMemberPrefix",
                        "Member property '" + prop.name + "' in class '" +
                        cls.name + "' should begin with 'm_' prefix (e.g. 'm_" +
                        prop.name + "') to distinguish internal state from local arguments.",
                        core::Severity::Info,
                        model::toSlang(prop.loc)
                    );
                }
            }
        }
    }
};

} // namespace uvmcheck::rules::style

namespace uvmcheck::rules {
std::unique_ptr<Rule> createClassMemberPrefix() {
    return std::make_unique<style::ClassMemberPrefix>();
}
}

UVMCHECK_REGISTER_RULE(uvmcheck::rules::createClassMemberPrefix)
