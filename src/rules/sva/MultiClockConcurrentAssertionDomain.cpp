#include "uvmcheck/rules/Rule.h"
#include "uvmcheck/rules/RuleRegistry.h"
#include "slang/syntax/AllSyntax.h"
#include "slang/syntax/SyntaxTree.h"
#include "slang/syntax/SyntaxVisitor.h"
#include <string>
#include <vector>
#include <set>

namespace uvmcheck::rules::sva {

class MultiClockPropertyVisitor : public engine::SyntaxVisitor<MultiClockPropertyVisitor> {
public:
    explicit MultiClockPropertyVisitor(core::DiagnosticEngine& diags) : diags(diags) {}

    void handle(const slang::syntax::PropertyDeclarationSyntax& prop) {
        checkNode(prop, prop.name.valueText(), prop.name.location());
        visitDefault(prop);
    }

private:
    core::DiagnosticEngine& diags;

    void checkNode(const slang::syntax::SyntaxNode& node, std::string_view name, slang::SourceLocation loc) {
        struct ClockCollector : public slang::syntax::SyntaxVisitor<ClockCollector> {
            std::set<std::string> clockSignals;

            void handle(const slang::syntax::SignalEventExpressionSyntax& sig) {
                if (sig.expr) {
                    std::string clk = std::string(sig.expr->toString());
                    clk.erase(0, clk.find_first_not_of(" \t\n\r"));
                    if (size_t end = clk.find_last_not_of(" \t\n\r"); end != std::string::npos) {
                        clk.erase(end + 1);
                    }
                    if (!clk.empty()) {
                        clockSignals.insert(clk);
                    }
                }
                visitDefault(sig);
            }
        };

        ClockCollector collector;
        node.visit(collector);

        if (collector.clockSignals.size() > 1) {
            std::string clkList;
            for (const auto& c : collector.clockSignals) {
                if (!clkList.empty()) clkList += ", ";
                clkList += "'" + c + "'";
            }

            diags.addViolation(
                "SV-SVA-005",
                "MultiClockConcurrentAssertionDomain",
                "Property '" + std::string(name) + "' mixes multiple distinct clock domains (" + clkList +
                ") without explicit clock domain crossing (CDC) synchronization. Under IEEE 1800 § 16.13 and "
                "Under IEEE 1800 § 16.13 assertion rules, multi-clocked assertions without formal CDC qualification introduce simulation "
                "race conditions, non-deterministic sampling, and exponential formal verification state explosion.",
                core::Severity::Error,
                loc
            );
        }
    }
};

class MultiClockConcurrentAssertionDomain : public Rule {
public:
    std::string_view getId() const override { return "SV-SVA-005"; }
    std::string_view getName() const override { return "MultiClockConcurrentAssertionDomain"; }
    std::string_view getDescription() const override {
        return "Disallows concurrent assertions mixing multiple asynchronous clock domains without formal CDC qualification.";
    }
    std::string_view getCategory() const override { return "SVA"; }
    core::Severity getDefaultSeverity() const override { return core::Severity::Error; }

    engine::RuleTask check(engine::RuleContext& ctx) override {
        auto& diags = ctx.diags();
        MultiClockPropertyVisitor visitor(diags);
        co_await ctx.visitSyntax(visitor);
    }
};

} // namespace uvmcheck::rules::sva

namespace uvmcheck::rules {
std::unique_ptr<Rule> createMultiClockConcurrentAssertionDomain() {
    return std::make_unique<sva::MultiClockConcurrentAssertionDomain>();
}
} // namespace uvmcheck::rules
