#include "uvmcheck/rules/Rule.h"
#include "uvmcheck/rules/RuleRegistry.h"
#include "uvmcheck/model/SlangModel.h"
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

    // Reads only uvmcheck::model, not slang's AST.
    void run(slang::ast::Compilation& /*compilation*/,
             const uvmcheck::uvm::UvmModel& uvmModel,
             core::DiagnosticEngine& diags) override {
        const model::Model& design = uvmModel.facade();
        for (const model::Class& cls : design.classes()) {
            if (cls.isParameterized || cls.base == model::NoClass) continue;
            const model::Class& baseClass = design.cls(cls.base);

            for (const model::Method& sub : cls.methods) {
                if (sub.isExtern) continue; // as before the port: in-class bodies only
                if (sub.name == "new") continue;

                // The base class's own method of that name, when declared virtual
                const model::Method* bs = baseClass.findMethod(sub.name);
                if (!bs || !bs->declaredVirtual) continue;
                size_t derivedArgs = sub.arguments.size();
                size_t baseArgs = bs->arguments.size();
                if (derivedArgs != baseArgs) {
                    diags.addViolation(
                        "SV-OOP-015",
                        "VirtualMethodSignatureMismatch",
                        "Method '" + sub.name + "' in derived class '" + cls.name +
                        "' defines " + std::to_string(derivedArgs) + " parameter(s), but overrides virtual method in base class '" +
                        baseClass.name + "' defined with " + std::to_string(baseArgs) + " parameter(s). In SystemVerilog OOP "
                        "(IEEE 1800 § 8.20), virtual method overrides must match base signature parameter counts; "
                        "mismatching arguments causes method hiding instead of polymorphic override.",
                        core::Severity::Warning,
                        model::toSlang(sub.loc)
                    );
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

UVMCHECK_REGISTER_RULE(uvmcheck::rules::createVirtualMethodSignatureMismatch)
