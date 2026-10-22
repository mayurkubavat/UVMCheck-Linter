#include "uvmcheck/rules/Rule.h"
#include "uvmcheck/rules/RuleRegistry.h"
#include "slang/syntax/SyntaxVisitor.h"
#include "slang/syntax/AllSyntax.h"
#include "slang/syntax/SyntaxTree.h"

namespace uvmcheck::rules::sva {

class UnlabeledAssertionVisitor : public engine::SyntaxVisitor<UnlabeledAssertionVisitor> {
public:
    explicit UnlabeledAssertionVisitor(core::DiagnosticEngine& diags) : diags(diags) {}

    void handle(const slang::syntax::ImmediateAssertionStatementSyntax& stmt) {
        if (!stmt.label) {
            std::string kw = std::string(stmt.keyword.valueText());
            diags.addViolation(
                "SVA-LBL-001",
                "UnlabeledAssertion",
                "Immediate assertion '" + kw + "' lacks an explicit label prefix (e.g. 'check_val: " + kw + " (...)'). "
                "Unlabeled assertions impede automated regression failure triage, ticket assignment, and coverage correlation.",
                core::Severity::Warning,
                stmt.keyword.location()
            );
        }
        visitDefault(stmt);
    }

    void handle(const slang::syntax::ConcurrentAssertionStatementSyntax& stmt) {
        if (!stmt.label) {
            std::string kw = std::string(stmt.keyword.valueText());
            std::string pOrS = std::string(stmt.propertyOrSequence.valueText());
            std::string fullKw = kw + (!pOrS.empty() ? (" " + pOrS) : "");
            diags.addViolation(
                "SVA-LBL-001",
                "UnlabeledAssertion",
                "Concurrent assertion '" + fullKw + "' lacks an explicit label prefix (e.g. 'check_handshake: " + fullKw + " (...)'). "
                "Assertions without labels cannot be targeted by simulator severity/assertion control commands (+assert_filter, $assertoff).",
                core::Severity::Warning,
                stmt.keyword.location()
            );
        }
        visitDefault(stmt);
    }

private:
    core::DiagnosticEngine& diags;
};

class UnlabeledAssertion : public Rule {
public:
    std::string_view getId() const override { return "SVA-LBL-001"; }
    std::string_view getName() const override { return "UnlabeledAssertion"; }
    std::string_view getDescription() const override {
        return "Immediate and concurrent assertions must declare an explicit label prefix (e.g. 'check_lbl: assert ...'). "
               "Lacking labels makes regression failure triage, ticket routing, and assertion control commands difficult.";
    }
    std::string_view getCategory() const override { return "SVA"; }
    core::Severity getDefaultSeverity() const override { return core::Severity::Warning; }

    engine::RuleTask check(engine::RuleContext& ctx) override {
        auto& diags = ctx.diags();
        UnlabeledAssertionVisitor visitor(diags);
        co_await ctx.visitSyntax(visitor);
    }
};

} // namespace uvmcheck::rules::sva

namespace uvmcheck::rules {
std::unique_ptr<Rule> createUnlabeledAssertion() {
    return std::make_unique<sva::UnlabeledAssertion>();
}
}
