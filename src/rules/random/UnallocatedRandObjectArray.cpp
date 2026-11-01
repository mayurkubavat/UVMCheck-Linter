// SV-RND-023: a rand array of class handles whose elements are never created.

#include "uvmcheck/rules/Rule.h"
#include "uvmcheck/rules/RuleRegistry.h"

#include "slang/ast/ASTVisitor.h"
#include "slang/ast/expressions/AssignmentExpressions.h"
#include "slang/ast/expressions/CallExpression.h"
#include "slang/ast/expressions/SelectExpressions.h"
#include "slang/ast/symbols/ClassSymbols.h"
#include "slang/ast/types/AllTypes.h"

#include <unordered_set>
#include <vector>

namespace uvmcheck::rules::random {

using namespace slang::ast;

namespace {

class RandArrayVisitor : public engine::AstVisitor<RandArrayVisitor> {
public:
    std::vector<const ClassPropertySymbol*> candidates;
    std::unordered_set<const Symbol*> populated;

    void handle(const ClassPropertySymbol& prop) {
        if (prop.randMode != RandMode::None) {
            const Type& t = prop.getType().getCanonicalType();
            if (t.isUnpackedArray()) {
                if (const Type* elem = t.getArrayElementType(); elem && elem->isClass()) candidates.push_back(&prop);
            }
        }
        visitDefault(prop);
    }

    // arr[i] = new / T::type_id::create(...) / some_handle
    void handle(const AssignmentExpression& assign) {
        if (assign.left().kind == ExpressionKind::ElementSelect) {
            if (const auto* sym = assign.left().as<ElementSelectExpression>().value().getSymbolReference())
                populated.insert(sym);
        }
        visitDefault(assign);
    }

    // q.push_back(obj), q.push_front(obj), q.insert(i, obj)
    void handle(const CallExpression& call) {
        auto name = call.getSubroutineName();
        if (call.isSystemCall() && (name == "push_back" || name == "push_front" || name == "insert") &&
            !call.arguments().empty()) {
            if (const auto* sym = call.arguments()[0]->getSymbolReference()) populated.insert(sym);
        }
        visitDefault(call);
    }
};

} // namespace

class UnallocatedRandObjectArray : public Rule {
public:
    std::string_view getId() const override { return "SV-RND-023"; }
    std::string_view getName() const override { return "UnallocatedRandObjectArray"; }
    std::string_view getDescription() const override {
        return "Flags rand arrays of class handles whose elements are never created. randomize() sizes "
               "a dynamic array but does not construct objects, so every element stays null and its "
               "rand members are never randomized.";
    }
    std::string_view getCategory() const override { return "Randomization"; }
    core::Severity getDefaultSeverity() const override { return core::Severity::Error; }

    engine::RuleTask check(engine::RuleContext& ctx) override {
        RandArrayVisitor visitor;
        co_await ctx.visitAst(visitor);
        for (const auto* prop : visitor.candidates) {
            if (visitor.populated.count(prop)) continue;
            ctx.diags().addViolation(
                getId(), getName(),
                "rand array '" + std::string(prop->name) + "' holds class handles, but no element is ever "
                "created (no '" + std::string(prop->name) + "[i] = new' or type_id::create). randomize() "
                "leaves the elements null, so their contents are never randomized. Create the elements, "
                "for example in pre_randomize() after sizing the array.",
                core::Severity::Error, prop->location);
        }
    }
};

} // namespace uvmcheck::rules::random

namespace uvmcheck::rules {
std::unique_ptr<Rule> createUnallocatedRandObjectArray() {
    return std::make_unique<random::UnallocatedRandObjectArray>();
}
} // namespace uvmcheck::rules

UVMCHECK_REGISTER_RULE(uvmcheck::rules::createUnallocatedRandObjectArray)
