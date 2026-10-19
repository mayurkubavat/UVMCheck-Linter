#include "uvmcheck/rules/Rule.h"
#include "uvmcheck/rules/RuleRegistry.h"
#include "slang/ast/symbols/ClassSymbols.h"
#include "slang/ast/symbols/SubroutineSymbols.h"

namespace uvmcheck::rules::oop {

class NonVirtualMethodOverride : public Rule {
public:
    std::string_view getId() const override { return "SV-OOP-001"; }
    std::string_view getName() const override { return "NonVirtualMethodOverride"; }
    std::string_view getDescription() const override {
        return "Overriding a non-virtual method in a derived class breaks polymorphism when accessed "
               "via a base class handle.";
    }
    std::string_view getCategory() const override { return "OOP"; }
    core::Severity getDefaultSeverity() const override { return core::Severity::Warning; }

    void run(slang::ast::Compilation&,
             const uvmcheck::uvm::UvmModel& uvmModel,
             core::DiagnosticEngine& diags) override {
        for (const auto* classType : uvmModel.getAllClasses()) {
            const auto* base = classType->getBaseClass();
            if (!base || !base->isClass()) continue;

            for (const auto& sub : classType->membersOfType<slang::ast::SubroutineSymbol>()) {
                if (sub.name == "new") continue;
                if (sub.flags.has(slang::ast::MethodFlags::BuiltIn)) continue;

                // Check ancestor classes
                const slang::ast::Type* ancestor = base;
                while (ancestor && ancestor->isClass()) {
                    const auto& ancClass = ancestor->as<slang::ast::ClassType>();
                    const auto* sym = ancClass.find(sub.name);

                    if (sym && sym->kind == slang::ast::SymbolKind::Subroutine) {
                        const auto& baseMethod = sym->as<slang::ast::SubroutineSymbol>();
                        if (baseMethod.flags.has(slang::ast::MethodFlags::BuiltIn)) break;

                        if (!baseMethod.isVirtual()) {
                            diags.addViolation(
                                "SV-OOP-001",
                                "NonVirtualMethodOverride",
                                "Method '" + std::string(sub.name) + "' in class '" +
                                std::string(classType->name) + "' overrides non-virtual method in base class '" +
                                std::string(ancClass.name) + "'.",
                                core::Severity::Warning,
                                sub.location
                            );
                        }
                        break; // Stop after finding immediate ancestor definition
                    }
                    ancestor = ancClass.getBaseClass();
                }
            }
        }
    }
};

} // namespace uvmcheck::rules::oop

namespace uvmcheck::rules {
std::unique_ptr<Rule> createNonVirtualMethodOverride() {
    return std::make_unique<oop::NonVirtualMethodOverride>();
}
}
