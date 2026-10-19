#include "uvmcheck/rules/Rule.h"
#include "uvmcheck/rules/RuleRegistry.h"
#include "slang/ast/symbols/ClassSymbols.h"
#include "slang/ast/symbols/SubroutineSymbols.h"
#include "slang/ast/symbols/VariableSymbols.h"

namespace uvmcheck::rules::oop {

class VariableShadowing : public Rule {
public:
    std::string_view getId() const override { return "SV-OOP-004"; }
    std::string_view getName() const override { return "VariableShadowing"; }
    std::string_view getDescription() const override {
        return "Detects variable shadowing where a derived class property shadows a base class property, "
               "or a method argument / local variable shadows a class member property.";
    }
    std::string_view getCategory() const override { return "OOP"; }
    core::Severity getDefaultSeverity() const override { return core::Severity::Warning; }

    void run(slang::ast::Compilation& /*compilation*/,
             const uvmcheck::uvm::UvmModel& uvmModel,
             core::DiagnosticEngine& diags) override {
        for (const auto* cls : uvmModel.getAllClasses()) {
            if (!cls) continue;

            // 1. Check derived class property shadowing base class property
            for (const auto& member : cls->members()) {
                if (member.kind == slang::ast::SymbolKind::ClassProperty) {
                    const auto& prop = member.as<slang::ast::ClassPropertySymbol>();

                    const slang::ast::Type* base = cls->getBaseClass();
                    while (base) {
                        if (base->isClass()) {
                            const auto& baseClass = base->as<slang::ast::ClassType>();
                            const auto* shadow = baseClass.find(prop.name);
                            if (shadow && shadow->kind == slang::ast::SymbolKind::ClassProperty) {
                                diags.addViolation(
                                    "SV-OOP-004",
                                    "VariableShadowing",
                                    "Class property '" + std::string(prop.name) + "' in derived class '" +
                                    std::string(cls->name) + "' shadows property of the same name in base class '" +
                                    std::string(baseClass.name) + "'. Accessing through a base class handle will reference the base member.",
                                    core::Severity::Warning,
                                    prop.location
                                );
                                break;
                            }
                            base = baseClass.getBaseClass();
                        } else {
                            break;
                        }
                    }
                }
                // 2. Check method argument or local variable shadowing class property
                else if (member.kind == slang::ast::SymbolKind::Subroutine) {
                    const auto& sub = member.as<slang::ast::SubroutineSymbol>();
                    if (sub.flags.has(slang::ast::MethodFlags::BuiltIn)) continue;

                    // Arguments (skip constructor arguments where names like name/parent are conventional)
                    if (sub.name != "new") {
                        for (const auto* arg : sub.getArguments()) {
                            const auto* classProp = cls->find(arg->name);
                            if (classProp && classProp->kind == slang::ast::SymbolKind::ClassProperty) {
                                diags.addViolation(
                                    "SV-OOP-004",
                                    "VariableShadowing",
                                    "Method argument '" + std::string(arg->name) + "' in method '" +
                                    std::string(sub.name) + "' shadows class property '" +
                                    std::string(classProp->name) + "'. Use 'this." + std::string(classProp->name) +
                                    "' or rename the argument to prevent silent assignment bugs.",
                                    core::Severity::Warning,
                                    arg->location
                                );
                            }
                        }
                    }

                    // Local variables inside method
                    for (const auto& localMember : sub.members()) {
                        if (localMember.kind == slang::ast::SymbolKind::Variable) {
                            const auto& var = localMember.as<slang::ast::VariableSymbol>();
                            const auto* classProp = cls->find(var.name);
                            if (classProp && classProp->kind == slang::ast::SymbolKind::ClassProperty) {
                                diags.addViolation(
                                    "SV-OOP-004",
                                    "VariableShadowing",
                                    "Local variable '" + std::string(var.name) + "' in method '" +
                                    std::string(sub.name) + "' shadows class property '" +
                                    std::string(classProp->name) + "'. Rename the local variable to avoid confusion.",
                                    core::Severity::Warning,
                                    var.location
                                );
                            }
                        }
                    }
                }
            }
        }
    }
};

} // namespace uvmcheck::rules::oop

namespace uvmcheck::rules {
std::unique_ptr<Rule> createVariableShadowing() {
    return std::make_unique<oop::VariableShadowing>();
}
}
