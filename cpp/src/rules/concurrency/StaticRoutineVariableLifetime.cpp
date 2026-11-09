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
        // Only tasks with static default lifetime outside of classes. A
        // function returns in zero time, so its calls cannot overlap and a
        // static local is harmless there (34 of the 36 findings in
        // Accellera's UVM library were such functions). In a task it is a
        // hazard, not a certain bug: a counter kept across calls on purpose
        // looks the same, so this is a warning.
        if (sub.subroutineKind == slang::ast::SubroutineKind::Task &&
            sub.defaultLifetime == slang::ast::VariableLifetime::Static) {
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
                                "Local variable '" + std::string(var.name) + "' in task '" +
                                std::string(sub.name) + "' is static: a task outside a class has static "
                                "lifetime by default, so calls that overlap in time share '" + std::string(var.name) +
                                "'. Declare the task 'automatic'.",
                                core::Severity::Warning,
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
        return "A task outside a class has static lifetime by default: its local variables are shared by "
               "every call that is running, so calls that overlap in time overwrite each other's values. "
               "Functions return in zero time and are not reported.";
    }
    std::string_view getCategory() const override { return "Concurrency"; }
    core::Severity getDefaultSeverity() const override { return core::Severity::Warning; }

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

UVMCHECK_REGISTER_RULE(uvmcheck::rules::createStaticRoutineVariableLifetime)
