#include "uvmcheck/rules/Rule.h"
#include "uvmcheck/rules/RuleRegistry.h"
#include "uvmcheck/rules/SweSupport.h"
#include "slang/ast/expressions/CallExpression.h"
#include "slang/ast/expressions/LiteralExpressions.h"
#include <string>

namespace uvmcheck::rules::uvm {

using namespace slang::ast;

class ObjectionWithoutDescriptionVisitor : public swe::UserCodeVisitor<ObjectionWithoutDescriptionVisitor> {
public:
    using UserCodeVisitor::handle;
    explicit ObjectionWithoutDescriptionVisitor(core::DiagnosticEngine& diags) : diags(diags) {}

    void handle(const CallExpression& call) {
        const auto name = call.getSubroutineName();
        if ((name == "raise_objection" || name == "drop_objection") && onPhaseOrObjection(call) &&
            !hasDescription(call)) {
            diags.addViolation(
                "UVM-OBJ-005", "ObjectionWithoutDescription",
                std::string(name) + "() has no description. +UVM_OBJECTION_TRACE then shows only the "
                    "component, not what it is waiting for. Pass one, e.g. "
                    "phase." + std::string(name) + "(this, \"main traffic sequence\").",
                core::Severity::Info, call.sourceRange.start());
        }
        visitDefault(call);
    }

private:
    static bool onPhaseOrObjection(const CallExpression& call) {
        const Expression* recv = call.thisClass();
        if (!recv || !recv->type) return false;
        const auto* cls = swe::asClass(*recv->type);
        if (!cls) return false;
        const auto base = swe::uvmBaseName(*cls);
        return cls->name == "uvm_phase" || cls->name == "uvm_objection" || base == "uvm_phase" ||
               base == "uvm_objection";
    }

    // The second argument is the description; an empty literal counts as none.
    static bool hasDescription(const CallExpression& call) {
        const int written = swe::positionalArgCount(call);
        if (written < 0) return true; // named arguments: leave them alone
        if (written < 2) return false;
        const auto args = call.arguments();
        if (args.size() < 2 || !args[1]) return true;
        const Expression& desc = swe::stripConversions(*args[1]);
        return desc.kind != ExpressionKind::StringLiteral || !desc.as<StringLiteral>().getValue().empty();
    }

    core::DiagnosticEngine& diags;
};

class ObjectionWithoutDescription : public Rule {
public:
    std::string_view getId() const override { return "UVM-OBJ-005"; }
    std::string_view getName() const override { return "ObjectionWithoutDescription"; }
    std::string_view getDescription() const override {
        return "Flags raise_objection()/drop_objection() on a phase or objection with no description "
               "or an empty one. A short description makes objection traces of a hung test readable.";
    }
    std::string_view getCategory() const override { return "UVM"; }
    core::Severity getDefaultSeverity() const override { return core::Severity::Info; }

    engine::RuleTask check(engine::RuleContext& ctx) override {
        ObjectionWithoutDescriptionVisitor visitor(ctx.diags());
        co_await ctx.visitAst(visitor);
    }
};

} // namespace uvmcheck::rules::uvm

namespace uvmcheck::rules {
std::unique_ptr<Rule> createObjectionWithoutDescription() {
    return std::make_unique<uvm::ObjectionWithoutDescription>();
}
} // namespace uvmcheck::rules

UVMCHECK_REGISTER_RULE(uvmcheck::rules::createObjectionWithoutDescription)
