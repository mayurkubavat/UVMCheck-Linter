#include "uvmcheck/rules/Rule.h"
#include "uvmcheck/rules/RuleRegistry.h"
#include "slang/ast/ASTVisitor.h"
#include "slang/ast/expressions/AssignmentExpressions.h"
#include "slang/ast/symbols/ClassSymbols.h"
#include "slang/ast/symbols/SubroutineSymbols.h"

namespace uvmcheck::rules::oop {

class SuperNewVisitor : public slang::ast::ASTVisitor<SuperNewVisitor, slang::ast::VisitFlags::AllGood> {
public:
    void handle(const slang::ast::NewClassExpression& expr) {
        if (expr.isSuperClass) {
            hasSuperNew = true;
        }
        visitDefault(expr);
    }

    bool foundSuperNew() const { return hasSuperNew; }

private:
    bool hasSuperNew = false;
};

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

    void run(slang::ast::Compilation&,
             const uvmcheck::uvm::UvmModel& uvmModel,
             core::DiagnosticEngine& diags) override {
        for (const auto* classType : uvmModel.getAllClasses()) {
            const auto* base = classType->getBaseClass();
            if (!base || !base->isClass()) continue;

            const auto* ctor = classType->getConstructor();
            if (!ctor || ctor->flags.has(slang::ast::MethodFlags::BuiltIn)) continue;

            SuperNewVisitor visitor;
            ctor->getBody().visit(visitor);

            if (!visitor.foundSuperNew()) {
                diags.addViolation(
                    "SV-OOP-003",
                    "MissingSuperNew",
                    "Constructor 'new' in class '" + std::string(classType->name) +
                    "' derives from '" + std::string(base->name) +
                    "' but does not explicitly call super.new(...).",
                    core::Severity::Warning,
                    ctor->location
                );
            }
        }
    }
};

} // namespace uvmcheck::rules::oop

namespace uvmcheck::rules {
std::unique_ptr<Rule> createMissingSuperNew() {
    return std::make_unique<oop::MissingSuperNew>();
}
}
