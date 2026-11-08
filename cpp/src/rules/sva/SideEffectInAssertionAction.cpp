#include "uvmcheck/rules/Rule.h"
#include "uvmcheck/rules/RuleRegistry.h"
#include "uvmcheck/rules/SweSupport.h"
#include "slang/ast/ASTVisitor.h"
#include "slang/ast/statements/MiscStatements.h"
#include <algorithm>
#include <string>
#include <unordered_set>
#include <vector>

namespace uvmcheck::rules::sva {

using namespace slang::ast;

// The block scopes a statement opens, so variables declared inside it can be told apart.
struct LocalBlocks : public ASTVisitor<LocalBlocks, VisitFlags::AllGood> {
    std::unordered_set<const Symbol*> blocks;
    void handle(const BlockStatement& block) {
        if (block.blockSymbol) blocks.insert(block.blockSymbol);
        visitDefault(block);
    }
};

class SideEffectInAssertionActionVisitor : public swe::UserCodeVisitor<SideEffectInAssertionActionVisitor> {
public:
    using UserCodeVisitor::handle;
    explicit SideEffectInAssertionActionVisitor(core::DiagnosticEngine& diags) : diags(diags) {}

    void handle(const ImmediateAssertionStatement& stmt) {
        check(stmt.ifTrue);
        check(stmt.ifFalse);
        visitDefault(stmt);
    }

    void handle(const ConcurrentAssertionStatement& stmt) {
        check(stmt.ifTrue);
        check(stmt.ifFalse);
        visitDefault(stmt);
    }

private:
    void check(const Statement* action) {
        if (!action) return;
        auto written = swe::writtenSymbols(*action);
        if (written.empty()) return;

        // Variables declared inside the action itself (such as the report object the UVM
        // reporting macros set up) disappear with it, so writing them changes nothing outside.
        LocalBlocks locals;
        action->visit(locals);

        // Name the variables in a stable order so the message does not depend on hashing.
        std::vector<std::string> names;
        for (const auto* s : written) {
            const Scope* scope = s->getParentScope();
            if (!scope || !locals.blocks.contains(&scope->asSymbol())) names.emplace_back(s->name);
        }
        if (names.empty()) return;
        std::sort(names.begin(), names.end());
        std::string list;
        for (const auto& n : names) list += (list.empty() ? "'" : ", '") + n + "'";

        diags.addViolation(
            "SVA-ACT-003", "SideEffectInAssertionAction",
            "Assertion action block changes " + list +
                ". When assertions are turned off the action never runs, so the testbench behaves "
                "differently. Keep action blocks to reporting, and update state outside the assertion.",
            core::Severity::Warning, action->sourceRange.start());
    }

    core::DiagnosticEngine& diags;
};

class SideEffectInAssertionAction : public Rule {
public:
    std::string_view getId() const override { return "SVA-ACT-003"; }
    std::string_view getName() const override { return "SideEffectInAssertionAction"; }
    std::string_view getDescription() const override {
        return "Flags assignments, increments and other writes inside the pass or fail action of an "
               "immediate or concurrent assertion. Reporting calls are fine; state changes vanish "
               "when assertions are disabled.";
    }
    std::string_view getCategory() const override { return "SVA"; }
    core::Severity getDefaultSeverity() const override { return core::Severity::Warning; }

    engine::RuleTask check(engine::RuleContext& ctx) override {
        SideEffectInAssertionActionVisitor visitor(ctx.diags());
        co_await ctx.visitAst(visitor);
    }
};

} // namespace uvmcheck::rules::sva

namespace uvmcheck::rules {
std::unique_ptr<Rule> createSideEffectInAssertionAction() {
    return std::make_unique<sva::SideEffectInAssertionAction>();
}
} // namespace uvmcheck::rules

UVMCHECK_REGISTER_RULE(uvmcheck::rules::createSideEffectInAssertionAction)
