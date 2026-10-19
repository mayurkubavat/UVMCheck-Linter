#include "uvmcheck/rules/Rule.h"
#include "uvmcheck/rules/RuleRegistry.h"
#include "uvmcheck/uvm/UvmModel.h"
#include "slang/ast/ASTVisitor.h"
#include "slang/ast/symbols/ClassSymbols.h"
#include "slang/ast/symbols/SubroutineSymbols.h"
#include "slang/ast/expressions/AssignmentExpressions.h"
#include "slang/syntax/AllSyntax.h"

namespace uvmcheck::rules::uvm {

class VirtualInterfaceModportDriveInversion : public Rule {
public:
    std::string_view getId() const override { return "UVM-VIF-DIR"; }
    std::string_view getName() const override { return "VirtualInterfaceModportDriveInversion"; }
    std::string_view getDescription() const override {
        return "Detects procedural signal driving (via '=' or '<=') on virtual interface handles inside "
               "passive uvm_monitor components. Monitors must remain purely passive observers to avoid "
               "multi-driver electrical contention and X-propagation.";
    }
    std::string_view getCategory() const override { return "UVM"; }
    core::Severity getDefaultSeverity() const override { return core::Severity::Error; }

    void run(slang::ast::Compilation& compilation,
             const uvmcheck::uvm::UvmModel& uvmModel,
             core::DiagnosticEngine& diags) override {

        class MonitorDriveVisitor : public slang::ast::ASTVisitor<MonitorDriveVisitor, slang::ast::VisitFlags::AllGood> {
        public:
            core::DiagnosticEngine& diags;
            std::string monitorClassName;

            MonitorDriveVisitor(core::DiagnosticEngine& d, std::string_view name)
                : diags(d), monitorClassName(name) {}

            void handle(const slang::ast::AssignmentExpression& assign) {
                if (assign.left().syntax) {
                    std::string lhsText(assign.left().syntax->toString());
                    // Check if assigning to vif.<signal> or intf.<signal>
                    if (lhsText.find("vif.") != std::string::npos ||
                        lhsText.find("intf.") != std::string::npos ||
                        lhsText.find("_vif.") != std::string::npos) {
                        
                        diags.addViolation(
                            "UVM-VIF-DIR",
                            "VirtualInterfaceModportDriveInversion",
                            "Passive uvm_monitor '" + monitorClassName + "' illegally drives virtual interface signal '" +
                            lhsText + "'. Monitors must be strictly read-only observers. Driving signals from a monitor "
                            "violates UVM passive component semantics and causes multi-driver contention.",
                            core::Severity::Error,
                            assign.sourceRange.start()
                        );
                    }
                }
                visitDefault(assign);
            }
        };

        for (const auto* cls : uvmModel.getAllClasses()) {
            if (!cls) continue;
            std::string clsName(cls->name);
            bool isMonitor = (clsName.find("monitor") != std::string::npos ||
                              uvmModel.isDerivedFrom(*cls, "uvm_monitor"));

            if (!isMonitor) continue;

            MonitorDriveVisitor visitor(diags, cls->name);
            for (const auto& member : cls->members()) {
                if (member.kind == slang::ast::SymbolKind::Subroutine) {
                    member.as<slang::ast::SubroutineSymbol>().getBody().visit(visitor);
                }
            }
        }
    }
};

} // namespace uvmcheck::rules::uvm

namespace uvmcheck::rules {
std::unique_ptr<Rule> createVirtualInterfaceModportDriveInversion() {
    return std::make_unique<uvm::VirtualInterfaceModportDriveInversion>();
}
}
