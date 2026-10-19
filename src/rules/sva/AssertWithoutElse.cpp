#include "uvmcheck/rules/Rule.h"
#include "uvmcheck/rules/RuleRegistry.h"
#include "slang/ast/ASTVisitor.h"
#include "slang/ast/statements/MiscStatements.h"

namespace uvmcheck::rules::sva {

class AssertWithoutElseVisitor : public slang::ast::ASTVisitor<AssertWithoutElseVisitor, slang::ast::VisitFlags::AllGood> {
public:
    explicit AssertWithoutElseVisitor(core::DiagnosticEngine& diags) : diags(diags) {}

    void handle(const slang::ast::ImmediateAssertionStatement& stmt) {
        if (stmt.assertionKind == slang::ast::AssertionKind::Assert && !stmt.ifFalse) {
            diags.addViolation(
                "SVA-ACT-001",
                "AssertWithoutElse",
                "Immediate assertion lacks an 'else' action block. Assertions without explicit action blocks "
                "(e.g. 'else `uvm_error(\"ASSERT_FAIL\", \"...\")') print generic default messages and bypass UVM report catchers.",
                core::Severity::Warning,
                stmt.sourceRange.start()
            );
        }
        visitDefault(stmt);
    }

    void handle(const slang::ast::ConcurrentAssertionStatement& stmt) {
        if (stmt.assertionKind == slang::ast::AssertionKind::Assert && !stmt.ifFalse) {
            diags.addViolation(
                "SVA-ACT-001",
                "AssertWithoutElse",
                "Concurrent assertion lacks an 'else' action block. When this assertion fails, the simulator emits "
                "a generic one-line diagnostic without severity filtering or payload context. Add 'else `uvm_error(...)'.",
                core::Severity::Warning,
                stmt.sourceRange.start()
            );
        }
        visitDefault(stmt);
    }

private:
    core::DiagnosticEngine& diags;
};

class AssertWithoutElse : public Rule {
public:
    std::string_view getId() const override { return "SVA-ACT-001"; }
    std::string_view getName() const override { return "AssertWithoutElse"; }
    std::string_view getDescription() const override {
        return "Immediate and concurrent assertions must include an 'else' action block (e.g. 'else `uvm_error(...)'). "
               "Assertions without action blocks produce unhelpful one-line messages and cannot integrate with UVM reporting.";
    }
    std::string_view getCategory() const override { return "SVA"; }
    core::Severity getDefaultSeverity() const override { return core::Severity::Warning; }

    void run(slang::ast::Compilation& compilation,
             const uvmcheck::uvm::UvmModel& /*uvmModel*/,
             core::DiagnosticEngine& diags) override {
        AssertWithoutElseVisitor visitor(diags);
        compilation.getRoot().visit(visitor);
    }
};

} // namespace uvmcheck::rules::sva

namespace uvmcheck::rules {
std::unique_ptr<Rule> createAssertWithoutElse() {
    return std::make_unique<sva::AssertWithoutElse>();
}
}
