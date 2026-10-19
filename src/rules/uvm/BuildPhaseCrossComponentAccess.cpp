#include "uvmcheck/rules/Rule.h"
#include "uvmcheck/rules/RuleRegistry.h"
#include "uvmcheck/uvm/UvmModel.h"
#include "slang/ast/ASTVisitor.h"
#include "slang/ast/symbols/ClassSymbols.h"
#include "slang/ast/expressions/MiscExpressions.h"
#include "slang/syntax/AllSyntax.h"

namespace uvmcheck::rules::uvm {

class BuildPhaseCrossComponentAccess : public Rule {
public:
    std::string_view getId() const override { return "UVM-PHS-010"; }
    std::string_view getName() const override { return "BuildPhaseCrossComponentAccess"; }
    std::string_view getDescription() const override {
        return "Detects direct access to sibling components or cross-component handles during build_phase. "
               "Because the execution order of build_phase among sibling components is undefined in UVM, "
               "referencing sibling state during build_phase results in non-deterministic initialization races.";
    }
    std::string_view getCategory() const override { return "UVM"; }
    core::Severity getDefaultSeverity() const override { return core::Severity::Warning; }

    void run(slang::ast::Compilation& compilation,
             const uvmcheck::uvm::UvmModel& uvmModel,
             core::DiagnosticEngine& diags) override {

        class BuildPhaseVisitor : public slang::ast::ASTVisitor<BuildPhaseVisitor, slang::ast::VisitFlags::AllGood> {
        public:
            const uvmcheck::uvm::UvmModel& model;
            core::DiagnosticEngine& diags;
            bool insideBuildPhase = false;

            BuildPhaseVisitor(const uvmcheck::uvm::UvmModel& m, core::DiagnosticEngine& d)
                : model(m), diags(d) {}

            void handle(const slang::ast::SubroutineSymbol& sub) {
                if (sub.name == "build_phase") {
                    bool prev = insideBuildPhase;
                    insideBuildPhase = true;
                    visitDefault(sub);
                    insideBuildPhase = prev;
                    return;
                }
                visitDefault(sub);
            }

            void handle(const slang::ast::MemberAccessExpression& expr) {
                if (insideBuildPhase && expr.syntax) {
                    std::string text(expr.syntax->toString());
                    // Detect expressions referencing sibling or parent's sibling:
                    // e.g. "m_parent.agent_a" or "parent.sibling" or "p.agent_b"
                    if (text.find("m_parent.") != std::string::npos ||
                        text.find("parent.") != std::string::npos) {
                        
                        // Check if it accesses a grandchild or sibling (has 2 or more dots)
                        size_t firstDot = text.find('.');
                        if (firstDot != std::string::npos && text.find('.', firstDot + 1) != std::string::npos) {
                            diags.addViolation(
                                "UVM-PHS-010",
                                "BuildPhaseCrossComponentAccess",
                                "Cross-component access '" + text + "' detected during build_phase. "
                                "UVM does not specify the execution order of build_phase across sibling components. "
                                "Move cross-component references to connect_phase or pass configurations using uvm_config_db.",
                                core::Severity::Warning,
                                expr.sourceRange.start()
                            );
                        }
                    }
                }
                visitDefault(expr);
            }
        };

        BuildPhaseVisitor visitor(uvmModel, diags);
        compilation.getRoot().visit(visitor);
    }
};

} // namespace uvmcheck::rules::uvm

namespace uvmcheck::rules {
std::unique_ptr<Rule> createBuildPhaseCrossComponentAccess() {
    return std::make_unique<uvm::BuildPhaseCrossComponentAccess>();
}
}
