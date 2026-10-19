#include "uvmcheck/rules/Rule.h"
#include "uvmcheck/rules/RuleRegistry.h"
#include "slang/ast/symbols/ClassSymbols.h"
#include "slang/ast/symbols/SubroutineSymbols.h"
#include <string>

namespace uvmcheck::rules::oop {

class VirtualMethodSignatureMismatch : public Rule {
public:
    std::string_view getId() const override { return "SV-OOP-015"; }
    std::string_view getName() const override { return "VirtualMethodSignatureMismatch"; }
    std::string_view getDescription() const override {
        return "Virtual method overrides in derived classes must match the parameter count and signature of the base virtual method.";
    }
    std::string_view getCategory() const override { return "OOP"; }
    core::Severity getDefaultSeverity() const override { return core::Severity::Warning; }

    void run(slang::ast::Compilation& /*compilation*/,
             const uvmcheck::uvm::UvmModel& uvmModel,
             core::DiagnosticEngine& diags) override {
        for (const auto* classType : uvmModel.getAllClasses()) {
            const auto* base = classType->getBaseClass();
            if (!base || !base->isClass()) continue;
            const auto& baseClass = base->as<slang::ast::ClassType>();

            for (const auto& sub : classType->membersOfType<slang::ast::SubroutineSymbol>()) {
                if (sub.name == "new") continue;

                // Look for matching method in base class hierarchy
                for (const auto& bs : baseClass.membersOfType<slang::ast::SubroutineSymbol>()) {
                    if (bs.name == sub.name && bs.flags.has(slang::ast::MethodFlags::Virtual)) {
                        size_t derivedArgs = sub.getArguments().size();
                        size_t baseArgs = bs.getArguments().size();
                        if (derivedArgs != baseArgs) {
                            diags.addViolation(
                                "SV-OOP-015",
                                "VirtualMethodSignatureMismatch",
                                "Method '" + std::string(sub.name) + "' in derived class '" + std::string(classType->name) +
                                "' defines " + std::to_string(derivedArgs) + " parameter(s), but overrides virtual method in base class '" +
                                std::string(base->name) + "' defined with " + std::to_string(baseArgs) + " parameter(s). In SystemVerilog OOP "
                                "(IEEE 1800 § 8.20), virtual method overrides must match base signature parameter counts; "
                                "mismatching arguments causes method hiding instead of polymorphic override.",
                                core::Severity::Warning,
                                sub.location
                            );
                        }
                        break;
                    }
                }
            }
        }
    }
};

} // namespace uvmcheck::rules::oop

namespace uvmcheck::rules {
std::unique_ptr<Rule> createVirtualMethodSignatureMismatch() {
    return std::make_unique<oop::VirtualMethodSignatureMismatch>();
}
} // namespace uvmcheck::rules
