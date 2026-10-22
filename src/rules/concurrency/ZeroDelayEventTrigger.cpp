#include "uvmcheck/rules/Rule.h"
#include "uvmcheck/rules/RuleRegistry.h"
#include "slang/syntax/AllSyntax.h"
#include "slang/syntax/SyntaxTree.h"
#include "slang/syntax/SyntaxVisitor.h"

namespace uvmcheck::rules::concurrency {

class ZeroDelayEventTriggerVisitor : public engine::SyntaxVisitor<ZeroDelayEventTriggerVisitor> {
public:
    explicit ZeroDelayEventTriggerVisitor(core::DiagnosticEngine& diags) : diags(diags) {}

    void handle(const slang::syntax::EventTriggerStatementSyntax& stmt) {
        if (stmt.kind == slang::syntax::SyntaxKind::BlockingEventTriggerStatement) {
            std::string eventName = std::string(stmt.name->getLastToken().valueText());

            std::vector<core::FixItHint> fixIts;
            if (stmt.trigger.range().start().valid()) {
                fixIts.push_back({
                    stmt.trigger.range(),
                    "->>",
                    "Replace blocking event trigger '->' with non-blocking event trigger '->>'"
                });
            }

            diags.addViolation(
                "SV-RCE-001",
                "ZeroDelayEventTrigger",
                "Blocking event trigger '-> " + eventName + "' executed in Active region. "
                "If waiting threads have not yet evaluated '@(" + eventName + ")' within the current time step, the trigger is permanently missed, creating a classic zero-delay race condition. "
                "Use non-blocking event trigger '->> " + eventName + "' to schedule the event in the NBA region, or use persistent event checking 'wait(" + eventName + ".triggered)' per IEEE 1800 § 15.5.",
                core::Severity::Warning,
                stmt.trigger.location(),
                std::move(fixIts)
            );
        }
        visitDefault(stmt);
    }

private:
    core::DiagnosticEngine& diags;
};

class ZeroDelayEventTrigger : public Rule {
public:
    std::string_view getId() const override { return "SV-RCE-001"; }
    std::string_view getName() const override { return "ZeroDelayEventTrigger"; }
    std::string_view getDescription() const override {
        return "Flags blocking event triggers ('-> event') which cause zero-delay simulation race conditions. "
               "Recommends non-blocking event triggers ('->> event') or persistent event evaluation ('wait(event.triggered)').";
    }
    std::string_view getCategory() const override { return "Concurrency"; }
    core::Severity getDefaultSeverity() const override { return core::Severity::Warning; }

    engine::RuleTask check(engine::RuleContext& ctx) override {
        auto& diags = ctx.diags();
        ZeroDelayEventTriggerVisitor visitor(diags);
        co_await ctx.visitSyntax(visitor);
    }
};

} // namespace uvmcheck::rules::concurrency

namespace uvmcheck::rules {
std::unique_ptr<Rule> createZeroDelayEventTrigger() {
    return std::make_unique<concurrency::ZeroDelayEventTrigger>();
}
}
