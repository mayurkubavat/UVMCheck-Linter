#include "uvmcheck/rules/Rule.h"
#include "uvmcheck/rules/RuleRegistry.h"
#include "slang/ast/ASTVisitor.h"
#include "slang/ast/symbols/SubroutineSymbols.h"
#include "slang/ast/symbols/VariableSymbols.h"

namespace uvmcheck::rules::concurrency {

class StaticRoutineVisitor : public engine::AstVisitor<StaticRoutineVisitor> {
public:
    explicit StaticRoutineVisitor(core::DiagnosticEngine& diags) : diags(diags) {}

    void handle(const slang::ast::SubroutineSymbol& sub) {
        // Only inspect routines with static default lifetime outside of classes
        if (sub.defaultLifetime == slang::ast::VariableLifetime::Static) {
            const auto* parentScope = sub.getParentScope();
            if (parentScope && parentScope->asSymbol().kind != slang::ast::SymbolKind::ClassType) {
                for (const auto& member : sub.members()) {
                    if (member.kind == slang::ast::SymbolKind::Variable) {
                        const auto& var = member.as<slang::ast::VariableSymbol>();
                        if (var.lifetime == slang::ast::VariableLifetime::Static &&
                            !var.flags.has(slang::ast::VariableFlags::Const) &&
                            !var.flags.has(slang::ast::VariableFlags::CompilerGenerated)) {
                            diags.addViolation(
                                "SV-THR-002",
                                "StaticRoutineVariableLifetime",
                                "Local variable '" + std::string(var.name) + "' in routine '" +
                                std::string(sub.name) + "' has implicit static lifetime. In non-class scopes, "
                                "tasks and functions default to static lifetime, causing local variables to be shared "
                                "across concurrent invocations and inducing race conditions. Declare routine as 'automatic'.",
                                core::Severity::Error,
                                var.location
                            );
                        }
                    }
                }
            }
        }
        visitDefault(sub);
    }

private:
    core::DiagnosticEngine& diags;
};

class StaticRoutineVariableLifetime : public Rule {
public:
    std::string_view getId() const override { return "SV-THR-002"; }
    std::string_view getName() const override { return "StaticRoutineVariableLifetime"; }
    std::string_view getDescription() const override {
        return "Non-class tasks and functions default to static lifetime in SystemVerilog. Local variables "
               "in static routines share memory addresses across concurrent calls, leading to intermittent corruption.";
    }
    std::string_view getCategory() const override { return "Concurrency"; }
    core::Severity getDefaultSeverity() const override { return core::Severity::Error; }

    engine::RuleTask check(engine::RuleContext& ctx) override {
        auto& diags = ctx.diags();
        StaticRoutineVisitor visitor(diags);
        co_await ctx.visitAst(visitor);
    }
};

} // namespace uvmcheck::rules::concurrency

namespace uvmcheck::rules {
std::unique_ptr<Rule> createStaticRoutineVariableLifetime() {
    return std::make_unique<concurrency::StaticRoutineVariableLifetime>();
}
}
