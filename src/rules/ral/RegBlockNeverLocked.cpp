// RAL-BLK-001: a top-level register block that is never locked.

#include "uvmcheck/rules/Rule.h"
#include "uvmcheck/rules/RuleRegistry.h"

#include "slang/ast/ASTVisitor.h"
#include "slang/ast/expressions/CallExpression.h"
#include "slang/ast/symbols/ClassSymbols.h"
#include "slang/ast/types/AllTypes.h"
#include "slang/syntax/SyntaxNode.h"

#include <unordered_set>
#include <vector>

namespace uvmcheck::rules::ral {

using namespace slang::ast;

namespace {

class LockVisitor : public engine::AstVisitor<LockVisitor> {
public:
    explicit LockVisitor(const uvmcheck::uvm::UvmModel& model) : model(model) {}

    std::vector<const ClassType*> blocks;
    std::unordered_set<const Type*> topUse, subUse, locked;

    bool isBlock(const Type& t) const {
        const Type& c = t.getCanonicalType();
        return c.kind == SymbolKind::ClassType && c.name != "uvm_reg_block" && model.isDerivedFrom(c, "uvm_reg_block");
    }

    void handle(const ClassType& cls) {
        if (isBlock(cls)) {
            blocks.push_back(&cls);
            // lock_model() called by the block on itself (in build(), say).
            if (const auto* syn = cls.getSyntax(); syn && syn->toString().find("lock_model") != std::string::npos)
                locked.insert(&cls);
        }
        visitDefault(cls);
    }

    void handle(const ClassPropertySymbol& prop) {
        const Type& t = prop.getType().getCanonicalType();
        if (isBlock(t)) {
            const Symbol& owner = prop.getParentScope()->asSymbol();
            const bool inBlock = owner.kind == SymbolKind::ClassType && isBlock(owner.as<Type>());
            (inBlock ? subUse : topUse).insert(&t);
        }
        visitDefault(prop);
    }

    void handle(const CallExpression& call) {
        if (!call.isSystemCall() && call.getSubroutineName() == "lock_model" && call.thisClass())
            locked.insert(&call.thisClass()->type->getCanonicalType());
        visitDefault(call);
    }

    bool related(const Type& a, const Type& b) const {
        return &a == &b || model.isDerivedFrom(a, b.name) || model.isDerivedFrom(b, a.name);
    }

private:
    const uvmcheck::uvm::UvmModel& model;
};

} // namespace

class RegBlockNeverLocked : public Rule {
public:
    std::string_view getId() const override { return "RAL-BLK-001"; }
    std::string_view getName() const override { return "RegBlockNeverLocked"; }
    std::string_view getDescription() const override {
        return "Flags a top-level register block (held by an env or test, not by another block) on which "
               "lock_model() is never called. Until the model is locked, address maps are not built, so "
               "frontdoor accesses and get_reg_by_offset() fail.";
    }
    std::string_view getCategory() const override { return "UVM"; }
    core::Severity getDefaultSeverity() const override { return core::Severity::Error; }

    engine::RuleTask check(engine::RuleContext& ctx) override {
        LockVisitor visitor(ctx.uvmModel());
        co_await ctx.visitAst(visitor);
        for (const auto* block : visitor.blocks) {
            bool top = false, sub = false, locked = false;
            for (const auto* t : visitor.topUse) top |= visitor.related(*block, *t);
            for (const auto* t : visitor.subUse) sub |= &block->getCanonicalType() == t;
            for (const auto* t : visitor.locked) locked |= visitor.related(*block, *t);
            if (!top || sub || locked) continue;
            ctx.diags().addViolation(
                getId(), getName(),
                "Register block '" + std::string(block->name) + "' is used as a top-level model but "
                "lock_model() is never called on it. Call lock_model() after build() (in the block's own "
                "build, or where the env creates it) so the address maps are finalized.",
                core::Severity::Error, block->location);
        }
    }
};

} // namespace uvmcheck::rules::ral

namespace uvmcheck::rules {
std::unique_ptr<Rule> createRegBlockNeverLocked() {
    return std::make_unique<ral::RegBlockNeverLocked>();
}
} // namespace uvmcheck::rules

UVMCHECK_REGISTER_RULE(uvmcheck::rules::createRegBlockNeverLocked)
