#include "uvmcheck/rules/Rule.h"
#include "uvmcheck/rules/RuleRegistry.h"
#include "slang/ast/ASTVisitor.h"
#include "slang/ast/symbols/BlockSymbols.h"
#include "slang/ast/statements/MiscStatements.h"
#include <string>

namespace uvmcheck::rules::concurrency {

class EventTriggerVisitor : public engine::AstVisitor<EventTriggerVisitor> {
public:
    explicit EventTriggerVisitor(core::DiagnosticEngine& diags) : diags(diags) {}

    void handle(const slang::ast::ProceduralBlockSymbol& proc) {
        if (proc.procedureKind == slang::ast::ProceduralBlockKind::AlwaysComb) {
            bool prev = inAlwaysComb;
            inAlwaysComb = true;
            proc.getBody().visit(*this);
            inAlwaysComb = prev;
        }
        visitDefault(proc);
    }

    void handle(const slang::ast::EventTriggerStatement& stmt) {
        if (inAlwaysComb) {
            diags.addViolation(
                "SV-CON-006",
                "EventTriggerInAlwaysComb",
                "Event trigger inside 'always_comb' block causes simulation delta-cycle oscillation and simulator lockup. Per IEEE 1800 § 9.2.2.2, combinational blocks must be purely functional and sensitive only to signal reads; triggering events produces infinite scheduling loops.",
                core::Severity::Error,
                stmt.sourceRange.start()
            );
        }
        visitDefault(stmt);
    }

private:
    core::DiagnosticEngine& diags;
    bool inAlwaysComb = false;
};

class EventTriggerInAlwaysComb : public Rule {
public:
    std::string_view getId() const override { return "SV-CON-006"; }
    std::string_view getName() const override { return "EventTriggerInAlwaysComb"; }
    std::string_view getDescription() const override {
        return "Disallows event triggers ('->' or '->>') inside always_comb blocks (IEEE 1800 § 9.2.2.2).";
    }
    std::string_view getCategory() const override { return "Concurrency"; }
    core::Severity getDefaultSeverity() const override { return core::Severity::Error; }

    engine::RuleTask check(engine::RuleContext& ctx) override {
        auto& diags = ctx.diags();
        EventTriggerVisitor visitor(diags);
        co_await ctx.visitAst(visitor);
    }
};

} // namespace uvmcheck::rules::concurrency

namespace uvmcheck::rules {
std::unique_ptr<Rule> createEventTriggerInAlwaysComb() {
    return std::make_unique<concurrency::EventTriggerInAlwaysComb>();
}
} // namespace uvmcheck::rules
