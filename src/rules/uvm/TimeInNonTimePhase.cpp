#include "uvmcheck/rules/Rule.h"
#include "uvmcheck/rules/RuleRegistry.h"
#include "slang/syntax/AllSyntax.h"
#include "slang/syntax/SyntaxTree.h"
#include "slang/syntax/SyntaxVisitor.h"
#include <string>
#include <unordered_set>

namespace uvmcheck::rules::uvm {

class TimingControlFinder : public slang::syntax::SyntaxVisitor<TimingControlFinder> {
public:
    const slang::syntax::SyntaxNode* foundNode = nullptr;
    std::string timingDescription;

    void handle(const slang::syntax::TimingControlStatementSyntax& stmt) {
        if (!foundNode) {
            foundNode = &stmt;
            timingDescription = "timing control (#, @, or ##)";
        }
        visitDefault(stmt);
    }

    void handle(const slang::syntax::WaitStatementSyntax& stmt) {
        if (!foundNode) {
            foundNode = &stmt;
            timingDescription = "wait (wait condition)";
        }
        visitDefault(stmt);
    }

    void handle(const slang::syntax::WaitForkStatementSyntax& stmt) {
        if (!foundNode) {
            foundNode = &stmt;
            timingDescription = "wait fork";
        }
        visitDefault(stmt);
    }
};

class TimeInNonTimePhaseVisitor : public slang::syntax::SyntaxVisitor<TimeInNonTimePhaseVisitor> {
public:
    explicit TimeInNonTimePhaseVisitor(core::DiagnosticEngine& diags) : diags(diags) {}

    void handle(const slang::syntax::FunctionDeclarationSyntax& func) {
        checkRoutine(func.prototype, func.items);
        visitDefault(func);
    }

private:
    void checkRoutine(const slang::syntax::FunctionPrototypeSyntax* proto,
                      const slang::syntax::SyntaxList<slang::syntax::SyntaxNode>& items) {
        if (!proto || !proto->name) {
            return;
        }

        static const std::unordered_set<std::string> kNonTimePhases = {
            "build_phase", "connect_phase", "end_of_elaboration_phase",
            "start_of_simulation_phase", "extract_phase", "check_phase",
            "report_phase", "final_phase"
        };

        std::string methodName(proto->name->getLastToken().valueText());
        if (kNonTimePhases.find(methodName) == kNonTimePhases.end()) {
            return;
        }

        TimingControlFinder finder;
        for (const auto* item : items) {
            if (item) {
                item->visit(finder);
                if (finder.foundNode) {
                    diags.addViolation(
                        "UVM-PHS-008",
                        "TimeInNonTimePhase",
                        "Non-time-consuming UVM phase '" + methodName +
                        "' contains time-consuming construct '" + finder.timingDescription +
                        "'. Non-run UVM phases execute as instantaneous functions in 0 simulation time. Timing controls inside these phases corrupt the UVM execution schedule.",
                        core::Severity::Error,
                        finder.foundNode->getFirstToken().location()
                    );
                    break;
                }
            }
        }
    }

    core::DiagnosticEngine& diags;
};

class TimeInNonTimePhase : public Rule {
public:
    std::string_view getId() const override { return "UVM-PHS-008"; }
    std::string_view getName() const override { return "TimeInNonTimePhase"; }
    std::string_view getDescription() const override {
        return "Disallows time-consuming statements (#, @, wait, ##) inside non-time-consuming UVM phases (e.g. build_phase, connect_phase, check_phase).";
    }
    std::string_view getCategory() const override { return "UVM"; }
    core::Severity getDefaultSeverity() const override { return core::Severity::Error; }

    void run(slang::ast::Compilation& compilation,
             const uvmcheck::uvm::UvmModel& /*uvmModel*/,
             core::DiagnosticEngine& diags) override {
        TimeInNonTimePhaseVisitor visitor(diags);
        for (const auto& tree : compilation.getSyntaxTrees()) {
            if (tree) {
                tree->root().visit(visitor);
            }
        }
    }
};

} // namespace uvmcheck::rules::uvm

namespace uvmcheck::rules {
std::unique_ptr<Rule> createTimeInNonTimePhase() {
    return std::make_unique<uvm::TimeInNonTimePhase>();
}
} // namespace uvmcheck::rules
