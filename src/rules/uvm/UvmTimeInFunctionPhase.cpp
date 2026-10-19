#include "uvmcheck/rules/Rule.h"
#include "uvmcheck/rules/RuleRegistry.h"
#include "slang/ast/ASTVisitor.h"
#include "slang/ast/statements/MiscStatements.h"
#include "slang/ast/symbols/SubroutineSymbols.h"
#include "slang/syntax/AllSyntax.h"
#include "slang/syntax/SyntaxVisitor.h"

namespace uvmcheck::rules::uvm {

class TimingControlVisitor : public slang::ast::ASTVisitor<TimingControlVisitor, slang::ast::VisitFlags::AllGood> {
public:
    void handle(const slang::ast::TimedStatement& stmt) {
        hasTiming = true;
        firstLoc = stmt.sourceRange.start();
        visitDefault(stmt);
    }

    void handle(const slang::ast::WaitStatement& stmt) {
        hasTiming = true;
        firstLoc = stmt.sourceRange.start();
        visitDefault(stmt);
    }

    bool foundTiming() const { return hasTiming; }
    slang::SourceLocation getLocation() const { return firstLoc; }

private:
    bool hasTiming = false;
    slang::SourceLocation firstLoc;
};

class UvmTimeInFunctionPhase : public Rule {
public:
    std::string_view getId() const override { return "UVM-PHS-002"; }
    std::string_view getName() const override { return "UvmTimeInFunctionPhase"; }
    std::string_view getDescription() const override {
        return "Function-based UVM phases (e.g., build_phase, connect_phase) must not consume simulation "
               "time using delays (#), event controls (@), or wait statements.";
    }
    std::string_view getCategory() const override { return "UVM"; }
    core::Severity getDefaultSeverity() const override { return core::Severity::Error; }

    void run(slang::ast::Compilation&,
             const uvmcheck::uvm::UvmModel& uvmModel,
             core::DiagnosticEngine& diags) override {
        for (const auto* classType : uvmModel.getAllClasses()) {
            if (!uvmModel.isUvmComponent(*classType)) continue;

            for (const auto& sub : classType->membersOfType<slang::ast::SubroutineSymbol>()) {
                if (uvmModel.isFunctionPhase(sub.name)) {
                    bool hasTiming = false;
                    slang::SourceLocation timingLoc;

                    const auto& body = sub.getBody();
                    TimingControlVisitor visitor;
                    body.visit(visitor);
                    if (visitor.foundTiming()) {
                        hasTiming = true;
                        timingLoc = visitor.getLocation();
                    } else if (sub.getSyntax()) {
                        struct TimingSyntaxVisitor : public slang::syntax::SyntaxVisitor<TimingSyntaxVisitor> {
                            bool found = false;
                            slang::SourceLocation loc;
                            void handle(const slang::syntax::TimingControlStatementSyntax& s) {
                                found = true;
                                loc = s.sourceRange().start();
                            }
                            void handle(const slang::syntax::WaitStatementSyntax& s) {
                                found = true;
                                loc = s.sourceRange().start();
                            }
                        } synVisitor;
                        sub.getSyntax()->visit(synVisitor);
                        if (synVisitor.found) {
                            hasTiming = true;
                            timingLoc = synVisitor.loc;
                        }
                    }

                    if (hasTiming) {
                        diags.addViolation(
                            "UVM-PHS-002",
                            "UvmTimeInFunctionPhase",
                            "Time-consuming statement detected inside function phase '" +
                            std::string(sub.name) + "' in class '" +
                            std::string(classType->name) + "'.",
                            core::Severity::Error,
                            timingLoc
                        );
                    }
                }
            }
        }
    }
};

} // namespace uvmcheck::rules::uvm

namespace uvmcheck::rules {
std::unique_ptr<Rule> createUvmTimeInFunctionPhase() {
    return std::make_unique<uvm::UvmTimeInFunctionPhase>();
}
}
