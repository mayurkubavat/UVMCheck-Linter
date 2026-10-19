#include "uvmcheck/rules/Rule.h"
#include "uvmcheck/rules/RuleRegistry.h"
#include "slang/ast/ASTVisitor.h"
#include "slang/ast/statements/ConditionalStatements.h"
#include "slang/ast/statements/LoopStatements.h"

namespace uvmcheck::rules::deadcode {

class EmptyConditionalVisitor : public slang::ast::ASTVisitor<EmptyConditionalVisitor, slang::ast::VisitFlags::AllGood> {
public:
    explicit EmptyConditionalVisitor(core::DiagnosticEngine& diags) : diags(diags) {}

    void handle(const slang::ast::ConditionalStatement& stmt) {
        if (stmt.ifTrue.kind == slang::ast::StatementKind::Empty) {
            diags.addViolation(
                "SV-DED-003",
                "AccidentalEmptyConditional",
                "Conditional statement 'if (...)' has an empty branch body (stray semicolon ';'). "
                "The subsequent block will execute unconditionally regardless of whether the condition evaluates true or false. "
                "Remove the accidental semicolon after the condition.",
                core::Severity::Error,
                stmt.sourceRange.start()
            );
        }
        visitDefault(stmt);
    }

    void handle(const slang::ast::WhileLoopStatement& loop) {
        if (loop.body.kind == slang::ast::StatementKind::Empty) {
            diags.addViolation(
                "SV-DED-003",
                "AccidentalEmptyConditional",
                "Loop statement 'while (...)' has an empty body (stray semicolon ';'). "
                "This creates either an unintended zero-delay infinite busy-wait loop or causes subsequent blocks to execute unconditionally.",
                core::Severity::Error,
                loop.sourceRange.start()
            );
        }
        visitDefault(loop);
    }

private:
    core::DiagnosticEngine& diags;
};

class AccidentalEmptyConditional : public Rule {
public:
    std::string_view getId() const override { return "SV-DED-003"; }
    std::string_view getName() const override { return "AccidentalEmptyConditional"; }
    std::string_view getDescription() const override {
        return "Detects accidental semicolons immediately following if() or while() headers. "
               "An empty conditional branch causes subsequent blocks to execute unconditionally, introducing severe logic bugs.";
    }
    std::string_view getCategory() const override { return "DeadCode"; }
    core::Severity getDefaultSeverity() const override { return core::Severity::Error; }

    void run(slang::ast::Compilation& compilation,
             const uvmcheck::uvm::UvmModel&,
             core::DiagnosticEngine& diags) override {
        EmptyConditionalVisitor visitor(diags);
        compilation.getRoot().visit(visitor);
    }
};

} // namespace uvmcheck::rules::deadcode

namespace uvmcheck::rules {
std::unique_ptr<Rule> createAccidentalEmptyConditional() {
    return std::make_unique<deadcode::AccidentalEmptyConditional>();
}
}
