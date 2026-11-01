// UVM-REP-009: `uvm_info at UVM_NONE, which no verbosity setting can silence.

#include "uvmcheck/rules/Rule.h"
#include "uvmcheck/rules/RuleRegistry.h"

#include "slang/ast/ASTVisitor.h"
#include "slang/ast/expressions/CallExpression.h"
#include "slang/ast/expressions/ConversionExpression.h"
#include "slang/ast/expressions/LiteralExpressions.h"

#include <optional>

namespace uvmcheck::rules::uvm {

using namespace slang::ast;

namespace {

bool isVerbosityNone(const Expression* e) {
    while (e && e->kind == ExpressionKind::Conversion) e = &e->as<ConversionExpression>().operand();
    if (!e) return false;
    if (const auto* sym = e->getSymbolReference()) return sym->name == "UVM_NONE";
    if (e->kind == ExpressionKind::IntegerLiteral) return e->as<IntegerLiteral>().getValue().as<int64_t>() == std::optional<int64_t>(0);
    return false;
}

class InfoNoneVisitor : public engine::AstVisitor<InfoNoneVisitor> {
public:
    InfoNoneVisitor(const slang::SourceManager& sm, core::DiagnosticEngine& diags) : sm(sm), diags(diags) {}

    void handle(const CallExpression& call) {
        if (!call.isSystemCall() && call.getSubroutineName() == "uvm_report_info" && call.arguments().size() >= 3 &&
            isVerbosityNone(call.arguments()[2])) {
            // Point at the `uvm_info in user code, not inside the macro body.
            auto loc = sm.getFullyExpandedLoc(call.sourceRange.start());
            diags.addViolation(
                "UVM-REP-009", "InfoAtNoneVerbosity",
                "Informational message at UVM_NONE: it prints at every verbosity, +UVM_VERBOSITY cannot "
                "silence it, and it floods regression logs. Use UVM_LOW for messages that should normally "
                "show, and keep UVM_NONE for the few lines every run must print (a test's final verdict).",
                core::Severity::Info, loc);
        }
        visitDefault(call);
    }

private:
    const slang::SourceManager& sm;
    core::DiagnosticEngine& diags;
};

} // namespace

class InfoAtNoneVerbosity : public Rule {
public:
    std::string_view getId() const override { return "UVM-REP-009"; }
    std::string_view getName() const override { return "InfoAtNoneVerbosity"; }
    std::string_view getDescription() const override {
        return "Flags `uvm_info messages at UVM_NONE verbosity, which cannot be filtered with "
               "+UVM_VERBOSITY. Experimental: measuring how often real benches use UVM_NONE on purpose.";
    }
    std::string_view getCategory() const override { return "UVM"; }
    core::Severity getDefaultSeverity() const override { return core::Severity::Info; }
    RuleMaturity getMaturity() const override { return RuleMaturity::Experimental; }

    engine::RuleTask check(engine::RuleContext& ctx) override {
        InfoNoneVisitor visitor(*ctx.compilation().getSourceManager(), ctx.diags());
        co_await ctx.visitAst(visitor);
    }
};

} // namespace uvmcheck::rules::uvm

namespace uvmcheck::rules {
std::unique_ptr<Rule> createInfoAtNoneVerbosity() {
    return std::make_unique<uvm::InfoAtNoneVerbosity>();
}
} // namespace uvmcheck::rules

UVMCHECK_REGISTER_RULE(uvmcheck::rules::createInfoAtNoneVerbosity)
