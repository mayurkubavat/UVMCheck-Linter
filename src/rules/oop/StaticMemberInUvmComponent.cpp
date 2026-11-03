#include "uvmcheck/rules/Rule.h"
#include "uvmcheck/rules/RuleRegistry.h"
#include "uvmcheck/model/SlangModel.h"

namespace uvmcheck::rules::oop {

class StaticMemberInUvmComponent : public Rule {
public:
    std::string_view getId() const override { return "SV-OOP-007"; }
    std::string_view getName() const override { return "StaticMemberInUvmComponent"; }
    std::string_view getDescription() const override {
        return "Non-const static variable declared in a uvm_component subclass is shared across all instances "
               "simulation-wide. In multi-channel or multi-agent environments, instances will overwrite each other's state.";
    }
    std::string_view getCategory() const override { return "Architecture"; }
    core::Severity getDefaultSeverity() const override { return core::Severity::Error; }

    // Reads only uvmcheck::model, not slang's AST: the first rule on the facade.
    void run(slang::ast::Compilation&,
             const uvmcheck::uvm::UvmModel& uvmModel,
             core::DiagnosticEngine& diags) override {
        const model::Model& design = uvmModel.facade();
        for (const model::Class& cls : design.classes()) {
            if (!model::isComponent(cls.uvm)) continue;
            for (const model::Property& prop : cls.properties) {
                if (!prop.isStatic || prop.isConst) continue;
                diags.addViolation(
                    "SV-OOP-007",
                    "StaticMemberInUvmComponent",
                    "Non-const static member property '" + prop.name + "' in uvm_component class '" +
                    cls.name + "'. In SystemVerilog, static class members are shared across all instances "
                    "simulation-wide. In multi-channel environments (e.g. multiple agents), instances will overwrite each other's state "
                    "causing race conditions. Use an instance-scoped member or a dedicated singleton service instead.",
                    core::Severity::Error,
                    model::toSlang(prop.loc)
                );
            }
        }
    }
};

} // namespace uvmcheck::rules::oop

namespace uvmcheck::rules {
std::unique_ptr<Rule> createStaticMemberInUvmComponent() {
    return std::make_unique<oop::StaticMemberInUvmComponent>();
}
}

UVMCHECK_REGISTER_RULE(uvmcheck::rules::createStaticMemberInUvmComponent)
