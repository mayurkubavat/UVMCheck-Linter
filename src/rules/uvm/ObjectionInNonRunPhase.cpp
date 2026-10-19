#include "uvmcheck/rules/Rule.h"
#include "uvmcheck/rules/RuleRegistry.h"
#include "uvmcheck/uvm/UvmModel.h"
#include "slang/ast/ASTVisitor.h"
#include "slang/ast/expressions/CallExpression.h"
#include "slang/ast/symbols/SubroutineSymbols.h"
#include <string>

namespace uvmcheck::rules::uvm {

class ObjectionInNonRunPhaseVisitor : public slang::ast::ASTVisitor<ObjectionInNonRunPhaseVisitor, slang::ast::VisitFlags::AllGood> {
public:
    explicit ObjectionInNonRunPhaseVisitor(core::DiagnosticEngine& diags, std::string_view phaseName)
        : diags(diags), phaseName(phaseName) {}

    void handle(const slang::ast::CallExpression& call) {
        auto subName = call.getSubroutineName();
        if (subName == "raise_objection" || subName == "drop_objection") {
            diags.addViolation(
                "UVM-PHS-009",
                "ObjectionInNonRunPhase",
                "Objection method '." + std::string(subName) + "()' called inside function phase '" + std::string(phaseName) + "'. "
                "In UVM methodology (IEEE 1800.2 § 9.2), phase objections control progression and completion strictly "
                "during runtime phases ('run_phase' and scheduled sub-phases). Function phases execute synchronously in "
                "zero simulation time; raising or dropping objections in function phases is prohibited and corrupts UVM objection counting.",
                core::Severity::Error,
                call.sourceRange.start()
            );
        }
        visitDefault(call);
    }

private:
    core::DiagnosticEngine& diags;
    std::string_view phaseName;
};

class ObjectionInNonRunPhase : public Rule {
public:
    std::string_view getId() const override { return "UVM-PHS-009"; }
    std::string_view getName() const override { return "ObjectionInNonRunPhase"; }
    std::string_view getDescription() const override {
        return "Disallows calling raise_objection() or drop_objection() inside non-run function phases (e.g. build_phase, connect_phase). "
               "Objections are strictly reserved for runtime phases.";
    }
    std::string_view getCategory() const override { return "UVM"; }
    core::Severity getDefaultSeverity() const override { return core::Severity::Error; }

    void run(slang::ast::Compilation& /*compilation*/,
             const uvmcheck::uvm::UvmModel& uvmModel,
             core::DiagnosticEngine& diags) override {
        for (const auto* cls : uvmModel.getAllClasses()) {
            if (!cls) continue;
            for (const auto& member : cls->members()) {
                if (member.kind == slang::ast::SymbolKind::Subroutine) {
                    const auto& sub = member.as<slang::ast::SubroutineSymbol>();
                    if (uvmcheck::uvm::UvmModel::isFunctionPhase(sub.name)) {
                        ObjectionInNonRunPhaseVisitor visitor(diags, sub.name);
                        sub.getBody().visit(visitor);
                    }
                }
            }
        }
    }
};

} // namespace uvmcheck::rules::uvm

namespace uvmcheck::rules {
std::unique_ptr<Rule> createObjectionInNonRunPhase() {
    return std::make_unique<uvm::ObjectionInNonRunPhase>();
}
}
