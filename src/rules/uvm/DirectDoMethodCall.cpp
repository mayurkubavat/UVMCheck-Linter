// UVM-BAN-003: calling a do_* hook directly instead of the public method.

#include "uvmcheck/rules/Rule.h"
#include "uvmcheck/rules/RuleRegistry.h"

#include "slang/ast/ASTVisitor.h"
#include "slang/ast/expressions/CallExpression.h"
#include "slang/ast/symbols/SubroutineSymbols.h"
#include "slang/syntax/SyntaxNode.h"

#include <map>

namespace uvmcheck::rules::uvm {

using namespace slang::ast;

namespace {

const std::map<std::string_view, std::string_view>& publicMethodFor() {
    static const std::map<std::string_view, std::string_view> m{
        {"do_copy", "copy()"},     {"do_compare", "compare()"}, {"do_print", "print() or sprint()"},
        {"do_pack", "pack()"},     {"do_unpack", "unpack()"},   {"do_record", "record()"},
    };
    return m;
}

class DoCallVisitor : public engine::AstVisitor<DoCallVisitor> {
public:
    DoCallVisitor(const uvmcheck::uvm::UvmModel& model, core::DiagnosticEngine& diags) : model(model), diags(diags) {}

    void handle(const CallExpression& call) {
        if (!call.isSystemCall()) {
            const auto& sub = *std::get<0>(call.subroutine);
            auto it = publicMethodFor().find(sub.name);
            const auto* cls = sub.getParentScope() ? &sub.getParentScope()->asSymbol() : nullptr;
            // super.do_copy(rhs) inside an override is the intended chaining.
            const bool viaSuper = call.syntax && call.syntax->toString().find("super") != std::string::npos;
            if (it != publicMethodFor().end() && !viaSuper && cls && cls->kind == SymbolKind::ClassType &&
                model.isUvmObject(cls->as<Type>())) {
                diags.addViolation(
                    "UVM-BAN-003", "DirectDoMethodCall",
                    "Direct call to " + std::string(sub.name) + "(). The do_* methods are hooks that the UVM "
                    "policy classes call from " + std::string(it->second) + "; calling them directly skips the "
                    "policy object (comparer, printer, packer, recorder) and the field automation. Call " +
                    std::string(it->second) + " instead.",
                    core::Severity::Warning, call.sourceRange.start());
            }
        }
        visitDefault(call);
    }

private:
    const uvmcheck::uvm::UvmModel& model;
    core::DiagnosticEngine& diags;
};

} // namespace

class DirectDoMethodCall : public Rule {
public:
    std::string_view getId() const override { return "UVM-BAN-003"; }
    std::string_view getName() const override { return "DirectDoMethodCall"; }
    std::string_view getDescription() const override {
        return "Flags direct calls to do_copy, do_compare, do_print, do_pack, do_unpack and do_record. "
               "These are hooks for the public copy/compare/print/pack/record methods; calling them "
               "directly bypasses the policy object. super.do_*() inside an override is allowed.";
    }
    std::string_view getCategory() const override { return "UVM"; }
    core::Severity getDefaultSeverity() const override { return core::Severity::Warning; }

    engine::RuleTask check(engine::RuleContext& ctx) override {
        DoCallVisitor visitor(ctx.uvmModel(), ctx.diags());
        co_await ctx.visitAst(visitor);
    }
};

} // namespace uvmcheck::rules::uvm

namespace uvmcheck::rules {
std::unique_ptr<Rule> createDirectDoMethodCall() {
    return std::make_unique<uvm::DirectDoMethodCall>();
}
} // namespace uvmcheck::rules

UVMCHECK_REGISTER_RULE(uvmcheck::rules::createDirectDoMethodCall)
