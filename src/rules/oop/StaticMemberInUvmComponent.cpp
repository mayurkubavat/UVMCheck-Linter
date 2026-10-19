#include "uvmcheck/rules/Rule.h"
#include "uvmcheck/rules/RuleRegistry.h"
#include "slang/ast/symbols/ClassSymbols.h"
#include "slang/ast/symbols/VariableSymbols.h"

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

    void run(slang::ast::Compilation&,
             const uvmcheck::uvm::UvmModel& uvmModel,
             core::DiagnosticEngine& diags) override {
        for (const auto* comp : uvmModel.getAllClasses()) {
            if (!comp || !uvmModel.isUvmComponent(*comp)) continue;

            for (const auto& member : comp->members()) {
                if (member.kind == slang::ast::SymbolKind::ClassProperty) {
                    const auto& prop = member.as<slang::ast::ClassPropertySymbol>();
                    if (prop.lifetime == slang::ast::VariableLifetime::Static &&
                        !prop.flags.has(slang::ast::VariableFlags::Const)) {
                        diags.addViolation(
                            "SV-OOP-007",
                            "StaticMemberInUvmComponent",
                            "Non-const static member property '" + std::string(prop.name) + "' in uvm_component class '" +
                            std::string(comp->name) + "'. In SystemVerilog, static class members are shared across all instances "
                            "simulation-wide. In multi-channel environments (e.g. multiple agents), instances will overwrite each other's state "
                            "causing race conditions. Use an instance-scoped member or a dedicated singleton service instead.",
                            core::Severity::Error,
                            prop.location
                        );
                    }
                }
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
