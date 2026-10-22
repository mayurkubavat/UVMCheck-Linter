#include "uvmcheck/rules/Rule.h"
#include "uvmcheck/rules/RuleRegistry.h"
#include "slang/syntax/AllSyntax.h"
#include "slang/syntax/SyntaxTree.h"
#include "slang/syntax/SyntaxVisitor.h"
#include <string>

namespace uvmcheck::rules::sva {

class MixedClockEdgesVisitor : public engine::SyntaxVisitor<MixedClockEdgesVisitor> {
public:
    explicit MixedClockEdgesVisitor(core::DiagnosticEngine& diags) : diags(diags) {}

    void checkScope(const slang::syntax::SyntaxNode& node,
                    std::string_view constructName,
                    slang::SourceLocation loc) {
        struct ClockCollector : public slang::syntax::SyntaxVisitor<ClockCollector> {
            bool hasPosedge = false;
            bool hasNegedge = false;
            slang::SourceLocation firstLoc;

            void handle(const slang::syntax::SignalEventExpressionSyntax& ev) {
                if (ev.edge.kind == slang::parsing::TokenKind::PosEdgeKeyword) {
                    hasPosedge = true;
                    if (!firstLoc.valid()) firstLoc = ev.edge.location();
                } else if (ev.edge.kind == slang::parsing::TokenKind::NegEdgeKeyword) {
                    hasNegedge = true;
                    if (!firstLoc.valid()) firstLoc = ev.edge.location();
                }
                visitDefault(ev);
            }
        } collector;

        node.visit(collector);

        if (collector.hasPosedge && collector.hasNegedge) {
            diags.addViolation(
                "SV-SVA-003",
                "MixedClockEdgesInProperty",
                std::string(constructName) + " contains mixed clock edge triggers (both 'posedge' and 'negedge'). "
                "In SystemVerilog assertion verification (IEEE 1800 § 16.13), mixing positive and negative clock "
                "edges within the same assertion introduces half-cycle race conditions and causes severe state space "
                "explosion in formal verification (FPV) engines.",
                core::Severity::Warning,
                collector.firstLoc.valid() ? collector.firstLoc : loc
            );
        }
    }

    void handle(const slang::syntax::PropertyDeclarationSyntax& decl) {
        std::string name = "Property '" + std::string(decl.name.valueText()) + "'";
        checkScope(decl, name, decl.name.location());
        visitDefault(decl);
    }

    void handle(const slang::syntax::ConcurrentAssertionStatementSyntax& stmt) {
        std::string kw = std::string(stmt.keyword.valueText());
        checkScope(stmt, "Assertion '" + kw + " property'", stmt.keyword.location());
        visitDefault(stmt);
    }

private:
    core::DiagnosticEngine& diags;
};

class MixedClockEdgesInProperty : public Rule {
public:
    std::string_view getId() const override { return "SV-SVA-003"; }
    std::string_view getName() const override { return "MixedClockEdgesInProperty"; }
    std::string_view getDescription() const override {
        return "Flags SVA properties or assertions that mix both 'posedge' and 'negedge' clocking events.";
    }
    std::string_view getCategory() const override { return "SVA"; }
    core::Severity getDefaultSeverity() const override { return core::Severity::Warning; }

    engine::RuleTask check(engine::RuleContext& ctx) override {
        auto& diags = ctx.diags();
        MixedClockEdgesVisitor visitor(diags);
        co_await ctx.visitSyntax(visitor);
    }
};

} // namespace uvmcheck::rules::sva

namespace uvmcheck::rules {
std::unique_ptr<Rule> createMixedClockEdgesInProperty() {
    return std::make_unique<sva::MixedClockEdgesInProperty>();
}
} // namespace uvmcheck::rules
