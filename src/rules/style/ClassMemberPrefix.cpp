#include "uvmcheck/rules/Rule.h"
#include "uvmcheck/rules/RuleRegistry.h"
#include "slang/ast/symbols/ClassSymbols.h"

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

    void run(slang::ast::Compilation& /*compilation*/,
             const uvmcheck::uvm::UvmModel& uvmModel,
             core::DiagnosticEngine& diags) override {
        for (const auto* cls : uvmModel.getAllClasses()) {
            if (!cls || !uvmModel.isUvmComponent(*cls)) continue;

            // Skip internal UVM base classes
            if (cls->name.starts_with("uvm_")) continue;

            for (const auto& member : cls->members()) {
                if (member.kind != slang::ast::SymbolKind::ClassProperty) continue;

                const auto& prop = member.as<slang::ast::ClassPropertySymbol>();
                if (prop.flags.has(slang::ast::VariableFlags::Const)) continue;
                if (prop.lifetime == slang::ast::VariableLifetime::Static) continue;

                // Built-in / macro injected properties
                if (prop.name == "type_name" || prop.name == "type_id") continue;

                if (!prop.name.starts_with("m_")) {
                    diags.addViolation(
                        "SV-STY-001",
                        "ClassMemberPrefix",
                        "Member property '" + std::string(prop.name) + "' in class '" +
                        std::string(cls->name) + "' should begin with 'm_' prefix (e.g. 'm_" +
                        std::string(prop.name) + "') to distinguish internal state from local arguments.",
                        core::Severity::Info,
                        prop.location
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
