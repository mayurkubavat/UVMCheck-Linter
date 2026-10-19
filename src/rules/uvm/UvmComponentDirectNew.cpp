#include "uvmcheck/rules/Rule.h"
#include "uvmcheck/rules/RuleRegistry.h"
#include "slang/ast/ASTVisitor.h"
#include "slang/ast/expressions/AssignmentExpressions.h"

namespace uvmcheck::rules::uvm {

class UvmComponentDirectNewVisitor : public slang::ast::ASTVisitor<UvmComponentDirectNewVisitor, slang::ast::VisitFlags::AllGood> {
public:
    UvmComponentDirectNewVisitor(const uvmcheck::uvm::UvmModel& model, core::DiagnosticEngine& diags)
        : uvmModel(model), diags(diags) {}

    void handle(const slang::ast::NewClassExpression& expr) {
        if (!expr.isSuperClass && uvmModel.isUvmComponent(*expr.type)) {
            diags.addViolation(
                "UVM-FAC-001",
                "UvmComponentDirectNew",
                "Direct instantiation of uvm_component '" + std::string(expr.type->toString()) +
                "' using new(). Use type_id::create() to enable factory overrides.",
                core::Severity::Error,
                expr.sourceRange.start()
            );
        }
        visitDefault(expr);
    }

private:
    const uvmcheck::uvm::UvmModel& uvmModel;
    core::DiagnosticEngine& diags;
};

class UvmComponentDirectNew : public Rule {
public:
    std::string_view getId() const override { return "UVM-FAC-001"; }
    std::string_view getName() const override { return "UvmComponentDirectNew"; }
    std::string_view getDescription() const override {
        return "UVM components must be created using the UVM factory (type_name::type_id::create) "
               "instead of direct new() calls to support testbench overrides.";
    }
    std::string_view getCategory() const override { return "UVM"; }
    core::Severity getDefaultSeverity() const override { return core::Severity::Error; }

    void run(slang::ast::Compilation& compilation,
             const uvmcheck::uvm::UvmModel& uvmModel,
             core::DiagnosticEngine& diags) override {
        UvmComponentDirectNewVisitor visitor(uvmModel, diags);
        compilation.getRoot().visit(visitor);
    }
};

} // namespace uvmcheck::rules::uvm

namespace uvmcheck::rules {
std::unique_ptr<Rule> createUvmComponentDirectNew() {
    return std::make_unique<uvm::UvmComponentDirectNew>();
}
}
