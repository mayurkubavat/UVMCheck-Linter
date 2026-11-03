#include "uvmcheck/rules/Rule.h"
#include "uvmcheck/rules/RuleRegistry.h"
#include "uvmcheck/model/SlangModel.h"

namespace uvmcheck::rules::oop {

class MissingSuperNew : public Rule {
public:
    std::string_view getId() const override { return "SV-OOP-003"; }
    std::string_view getName() const override { return "MissingSuperNew"; }
    std::string_view getDescription() const override {
        return "Constructors in derived classes should explicitly invoke super.new(...) to ensure "
               "base class state is initialized.";
    }
    std::string_view getCategory() const override { return "OOP"; }
    core::Severity getDefaultSeverity() const override { return core::Severity::Warning; }

    // Reads only uvmcheck::model, not slang's AST.
    void run(slang::ast::Compilation&,
             const uvmcheck::uvm::UvmModel& uvmModel,
             core::DiagnosticEngine& diags) override {
        const model::Model& design = uvmModel.facade();
        for (const model::Class& cls : design.classes()) {
            if (cls.isParameterized) continue;
            if (cls.base == model::NoClass) continue;
            const model::Method* ctor = cls.findMethod("new");
            if (!ctor || ctor->isBuiltIn || ctor->callsSuperNew) continue;
            diags.addViolation(
                "SV-OOP-003",
                "MissingSuperNew",
                "Constructor 'new' in class '" + cls.name +
                "' derives from '" + design.cls(cls.base).name +
                "' but does not explicitly call super.new(...).",
                core::Severity::Warning,
                model::toSlang(ctor->loc)
            );
        }
    }
};

} // namespace uvmcheck::rules::oop

namespace uvmcheck::rules {
std::unique_ptr<Rule> createMissingSuperNew() {
    return std::make_unique<oop::MissingSuperNew>();
}
}

UVMCHECK_REGISTER_RULE(uvmcheck::rules::createMissingSuperNew)
