#include "uvmcheck/rules/Rule.h"
#include "uvmcheck/rules/RuleRegistry.h"
#include "uvmcheck/model/SlangModel.h"

namespace uvmcheck::rules::uvm {

class UvmConfigDbNullContext : public Rule {
public:
    std::string_view getId() const override { return "UVM-CFG-002"; }
    std::string_view getName() const override { return "UvmConfigDbNullContext"; }
    std::string_view getDescription() const override {
        return "Passing 'null' as context to uvm_config_db inside a uvm_component breaks hierarchical "
               "scoping and prevents targeted configuration overrides. Use 'this' instead.";
    }
    std::string_view getCategory() const override { return "UVM"; }
    core::Severity getDefaultSeverity() const override { return core::Severity::Warning; }

    // Reads only uvmcheck::model, not slang's AST.
    void run(slang::ast::Compilation&,
             const uvmcheck::uvm::UvmModel& uvmModel,
             core::DiagnosticEngine& diags) override {
        const model::Model& design = uvmModel.facade();
        for (const model::Class& cls : design.classes()) {
            if (cls.isParameterized) continue;
            if (!model::isComponent(cls.uvm)) continue;
            for (const model::Method& sub : cls.methods) {
                if (sub.isExtern || sub.isBuiltIn) continue;
                for (const model::Call& call : sub.calls) {
                    if (call.isSystem || (call.name != "get" && call.name != "set")) continue;
                    if (call.calleeScope.find("uvm_config_db") == std::string::npos || !call.firstArgIsNull) continue;
                    diags.addViolation(
                        "UVM-CFG-002",
                        "UvmConfigDbNullContext",
                        "Passing 'null' as context to uvm_config_db::" + call.name +
                        " inside uvm_component '" + cls.name +
                        "'. Pass 'this' instead to preserve hierarchical scoping.",
                        core::Severity::Warning,
                        model::toSlang(call.firstArgLoc)
                    );
                }
            }
        }
    }
};

} // namespace uvmcheck::rules::uvm

namespace uvmcheck::rules {
std::unique_ptr<Rule> createUvmConfigDbNullContext() {
    return std::make_unique<uvm::UvmConfigDbNullContext>();
}
}

UVMCHECK_REGISTER_RULE(uvmcheck::rules::createUvmConfigDbNullContext)
