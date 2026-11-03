#include "uvmcheck/rules/Rule.h"
#include "uvmcheck/rules/RuleRegistry.h"
#include "uvmcheck/model/SlangModel.h"

namespace uvmcheck::rules::oop {

class SuperNewNotFirstStatement : public Rule {
public:
    std::string_view getId() const override { return "SV-OOP-014"; }
    std::string_view getName() const override { return "SuperNewNotFirstStatement"; }
    std::string_view getDescription() const override {
        return "Derived class constructors calling super.new(...) must invoke it as the very first procedural statement.";
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
            if (!ctor || ctor->isBuiltIn || ctor->superNewIndex <= 0) continue;
            diags.addViolation(
                "SV-OOP-014",
                "SuperNewNotFirstStatement",
                "Constructor 'new' in derived class '" + cls.name +
                "' invokes 'super.new(...)' after other procedural statements. In SystemVerilog OOP "
                "(IEEE 1800 § 8.17), 'super.new(...)' must be the "
                "very first procedural statement in a derived class constructor to ensure base class properties "
                "and infrastructure are fully initialized before derived logic executes.",
                core::Severity::Warning,
                model::toSlang(ctor->superNewLoc)
            );
        }
    }
};

} // namespace uvmcheck::rules::oop

namespace uvmcheck::rules {
std::unique_ptr<Rule> createSuperNewNotFirstStatement() {
    return std::make_unique<oop::SuperNewNotFirstStatement>();
}
} // namespace uvmcheck::rules

UVMCHECK_REGISTER_RULE(uvmcheck::rules::createSuperNewNotFirstStatement)
