#include "uvmcheck/rules/Rule.h"
#include "uvmcheck/rules/RuleRegistry.h"
#include "slang/ast/ASTVisitor.h"
#include "slang/ast/symbols/BlockSymbols.h"
#include "slang/ast/statements/MiscStatements.h"

namespace uvmcheck::rules::sva {

class ImmediateAssertVisitor : public slang::ast::ASTVisitor<ImmediateAssertVisitor, slang::ast::VisitFlags::AllGood> {
public:
    explicit ImmediateAssertVisitor(core::DiagnosticEngine& diags) : diags(diags) {}

    void handle(const slang::ast::ImmediateAssertionStatement& stmt) {
        if (!stmt.isDeferred && !stmt.isFinal) {
            diags.addViolation(
                "SV-SVA-001",
                "ImmediateAssertInCombinatorialLogic",
                "Simple immediate assertion 'assert (...)' evaluated inside combinatorial process block. "
                "Immediate assertions in active combinatorial logic evaluate transient intermediate states (glitches) "
                "before signal values settle, triggering spurious false-positive failures. Use deferred assertion 'assert #0 (...)' or 'assert final (...)'.",
                core::Severity::Warning,
                stmt.sourceRange.start()
            );
        }
        visitDefault(stmt);
    }

private:
    core::DiagnosticEngine& diags;
};

class ProceduralAssertVisitor : public engine::AstVisitor<ProceduralAssertVisitor> {
public:
    explicit ProceduralAssertVisitor(core::DiagnosticEngine& diags) : diags(diags) {}

    void handle(const slang::ast::ProceduralBlockSymbol& proc) {
        if (proc.procedureKind == slang::ast::ProceduralBlockKind::AlwaysComb ||
            proc.procedureKind == slang::ast::ProceduralBlockKind::AlwaysLatch) {
            ImmediateAssertVisitor visitor(diags);
            proc.getBody().visit(visitor);
        }
        visitDefault(proc);
    }

private:
    core::DiagnosticEngine& diags;
};

class ImmediateAssertInCombinatorialLogic : public Rule {
public:
    std::string_view getId() const override { return "SV-SVA-001"; }
    std::string_view getName() const override { return "ImmediateAssertInCombinatorialLogic"; }
    std::string_view getDescription() const override {
        return "Simple immediate assertions in always_comb or combinatorial blocks evaluate transient delta glitches. "
               "Convert to deferred immediate assertions (assert #0 or assert final) to wait for settled circuit values.";
    }
    std::string_view getCategory() const override { return "SVA"; }
    core::Severity getDefaultSeverity() const override { return core::Severity::Warning; }

    engine::RuleTask check(engine::RuleContext& ctx) override {
        auto& diags = ctx.diags();
        ProceduralAssertVisitor visitor(diags);
        co_await ctx.visitAst(visitor);
    }
};

} // namespace uvmcheck::rules::sva

namespace uvmcheck::rules {
std::unique_ptr<Rule> createImmediateAssertInCombinatorialLogic() {
    return std::make_unique<sva::ImmediateAssertInCombinatorialLogic>();
}
}
