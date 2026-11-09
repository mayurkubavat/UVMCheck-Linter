#include "uvmcheck/rules/Rule.h"
#include "uvmcheck/rules/RuleRegistry.h"
#include "uvmcheck/model/SlangModel.h"

#include <string>
#include <utility>

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

    // Reads only uvmcheck::model, not slang's AST.
    void run(slang::ast::Compilation& /*compilation*/,
             const uvmcheck::uvm::UvmModel& uvmModel,
             core::DiagnosticEngine& diags) override {
        const model::Model& design = uvmModel.facade();
        for (const model::Class& cls : design.classes()) {
            if (cls.isParameterized) continue;
            const auto chain = design.ancestors(design.idOf(cls));

            // 1. Check derived class property shadowing base class property
            for (const model::Property& prop : cls.properties) {
                for (model::ClassId b : chain) {
                    const model::Class& baseClass = design.cls(b);
                    const model::Property* shadow = baseClass.findProperty(prop.name);
                    if (!shadow) continue;
                    auto* v = diags.addViolation(
                        "SV-OOP-004",
                        "VariableShadowing",
                        "Class property '" + prop.name + "' in derived class '" +
                        cls.name + "' shadows property of the same name in base class '" +
                        baseClass.name + "'. Accessing through a base class handle will reference the base member.",
                        core::Severity::Warning,
                        model::toSlang(prop.loc)
                    );
                    diags.addRelated(v, model::toSlang(shadow->loc), "shadowed property in '" + baseClass.name + "'");
                    break;
                }
            }

            // 2. Check method argument or local variable shadowing class property: its own,
            // or the nearest visible (not `local`) one it inherits. A static method sees no
            // instance property. The native rule looks up the same way.
            auto member = [&](const std::string& name, bool staticMethod)
                -> std::pair<const model::Property*, const model::Class*> {
                const model::Property* p = cls.findProperty(name);
                const model::Class* owner = &cls;
                if (!p) {
                    for (model::ClassId b : chain) {
                        const model::Class& base = design.cls(b);
                        if ((p = base.findProperty(name))) {
                            if (p->visibility == model::Visibility::Local) p = nullptr;
                            owner = &base;
                            break;
                        }
                    }
                }
                if (p && staticMethod && !p->isStatic) p = nullptr;
                return {p, owner};
            };
            auto which = [&](const model::Property& p, const model::Class& owner) {
                return &owner == &cls ? "'" + p.name + "'" : "'" + p.name + "' inherited from '" + owner.name + "'";
            };
            for (const model::Method& sub : cls.methods) {
                if (sub.isExtern) continue; // as before the port: in-class bodies only
                if (sub.isBuiltIn) continue;

                // Arguments (skip constructor arguments where names like name/parent are conventional)
                if (sub.name != "new") {
                    for (const model::Argument& arg : sub.arguments) {
                        const auto [classProp, owner] = member(arg.name, sub.isStatic);
                        if (!classProp) continue;
                        auto* v = diags.addViolation(
                            "SV-OOP-004",
                            "VariableShadowing",
                            "Method argument '" + arg.name + "' in method '" +
                            sub.name + "' shadows class property " +
                            which(*classProp, *owner) + ". Use 'this." + classProp->name +
                            "' or rename the argument to prevent silent assignment bugs.",
                            core::Severity::Warning,
                            model::toSlang(arg.loc)
                        );
                        diags.addRelated(v, model::toSlang(classProp->loc), "class property");
                    }
                }

                // Local variables inside method
                for (const model::Local& var : sub.locals) {
                    const auto [classProp, owner] = member(var.name, sub.isStatic);
                    if (!classProp) continue;
                    diags.addViolation(
                        "SV-OOP-004",
                        "VariableShadowing",
                        "Local variable '" + var.name + "' in method '" +
                        sub.name + "' shadows class property " +
                        which(*classProp, *owner) + ". Rename the local variable to avoid confusion.",
                        core::Severity::Warning,
                        model::toSlang(var.loc)
                    );
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

UVMCHECK_REGISTER_RULE(uvmcheck::rules::createVariableShadowing)
