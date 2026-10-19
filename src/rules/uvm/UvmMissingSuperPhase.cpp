#include "uvmcheck/rules/Rule.h"
#include "uvmcheck/rules/RuleRegistry.h"
#include "slang/ast/ASTVisitor.h"
#include "slang/ast/expressions/CallExpression.h"
#include "slang/ast/symbols/SubroutineSymbols.h"

namespace uvmcheck::rules::uvm {

class SuperCallCheckVisitor : public slang::ast::ASTVisitor<SuperCallCheckVisitor, slang::ast::VisitFlags::AllGood> {
public:
    explicit SuperCallCheckVisitor(std::string_view phaseName)
        : targetPhase(phaseName) {}

    void handle(const slang::ast::CallExpression& expr) {
        if (expr.getSubroutineName() == targetPhase) {
            hasSuperCall = true;
        }
        visitDefault(expr);
    }

    bool foundSuperCall() const { return hasSuperCall; }

private:
    std::string_view targetPhase;
    bool hasSuperCall = false;
};

class UvmMissingSuperPhase : public Rule {
public:
    std::string_view getId() const override { return "UVM-PHS-001"; }
    std::string_view getName() const override { return "UvmMissingSuperPhase"; }
    std::string_view getDescription() const override {
        return "Overridden standard UVM phases (e.g., build_phase, connect_phase) in uvm_component "
               "must call super.<phase_name>(phase) to ensure proper base-class configuration.";
    }
    std::string_view getCategory() const override { return "UVM"; }
    core::Severity getDefaultSeverity() const override { return core::Severity::Warning; }

    void run(slang::ast::Compilation&,
             const uvmcheck::uvm::UvmModel& uvmModel,
             core::DiagnosticEngine& diags) override {
        for (const auto* classType : uvmModel.getAllClasses()) {
            // Do not check the root uvm_component base class itself
            if (classType->name == "uvm_component") continue;
            if (!uvmModel.isUvmComponent(*classType)) continue;

            for (const auto& sub : classType->membersOfType<slang::ast::SubroutineSymbol>()) {
                if (uvmModel.isStandardUvmPhase(sub.name)) {
                    const auto& body = sub.getBody();

                    SuperCallCheckVisitor visitor(sub.name);
                    body.visit(visitor);

                    if (!visitor.foundSuperCall()) {
                        diags.addViolation(
                            "UVM-PHS-001",
                            "UvmMissingSuperPhase",
                            "Method '" + std::string(sub.name) + "' in class '" +
                            std::string(classType->name) + "' does not call super." +
                            std::string(sub.name) + "(phase).",
                            core::Severity::Warning,
                            sub.location
                        );
                    }
                }
            }
        }
    }
};

} // namespace uvmcheck::rules::uvm

namespace uvmcheck::rules {
std::unique_ptr<Rule> createUvmMissingSuperPhase() {
    return std::make_unique<uvm::UvmMissingSuperPhase>();
}
}
