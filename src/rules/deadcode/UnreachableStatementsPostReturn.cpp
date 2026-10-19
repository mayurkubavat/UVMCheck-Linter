#include "uvmcheck/rules/Rule.h"
#include "uvmcheck/rules/RuleRegistry.h"
#include "slang/ast/ASTVisitor.h"
#include "slang/ast/Statement.h"
#include "slang/ast/statements/MiscStatements.h"

namespace uvmcheck::rules::deadcode {

class UnreachableVisitor : public slang::ast::ASTVisitor<UnreachableVisitor, slang::ast::VisitFlags::AllGood> {
public:
    explicit UnreachableVisitor(core::DiagnosticEngine& diags) : diags(diags) {}

    void handle(const slang::ast::StatementList& stmtList) {
        bool terminated = false;
        std::string termKind;

        for (const auto* s : stmtList.list) {
            if (terminated) {
                if (s->kind != slang::ast::StatementKind::Empty) {
                    diags.addViolation(
                        "SV-DED-002",
                        "UnreachableStatementsPostReturn",
                        "Unreachable code statement following unconditional control-flow termination ('" +
                        termKind + "'). Subsequent statements in this block can never execute.",
                        core::Severity::Warning,
                        s->sourceRange.start()
                    );
                    break;
                }
            }

            if (s->kind == slang::ast::StatementKind::Return) {
                terminated = true;
                termKind = "return";
            } else if (s->kind == slang::ast::StatementKind::Break) {
                terminated = true;
                termKind = "break";
            } else if (s->kind == slang::ast::StatementKind::Continue) {
                terminated = true;
                termKind = "continue";
            }
        }
        visitDefault(stmtList);
    }

private:
    core::DiagnosticEngine& diags;
};

class UnreachableStatementsPostReturn : public Rule {
public:
    std::string_view getId() const override { return "SV-DED-002"; }
    std::string_view getName() const override { return "UnreachableStatementsPostReturn"; }
    std::string_view getDescription() const override {
        return "Detects unreachable statements appearing after an unconditional control-flow termination "
               "(return, break, continue). Dead code clutters verification environments and hides logic errors.";
    }
    std::string_view getCategory() const override { return "DeadCode"; }
    core::Severity getDefaultSeverity() const override { return core::Severity::Warning; }

    void run(slang::ast::Compilation& compilation,
             const uvmcheck::uvm::UvmModel&,
             core::DiagnosticEngine& diags) override {
        UnreachableVisitor visitor(diags);
        compilation.getRoot().visit(visitor);
    }
};

} // namespace uvmcheck::rules::deadcode

namespace uvmcheck::rules {
std::unique_ptr<Rule> createUnreachableStatementsPostReturn() {
    return std::make_unique<deadcode::UnreachableStatementsPostReturn>();
}
}
