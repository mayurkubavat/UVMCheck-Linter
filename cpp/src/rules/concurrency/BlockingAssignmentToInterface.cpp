#include "uvmcheck/rules/Rule.h"
#include "uvmcheck/rules/RuleRegistry.h"
#include "slang/syntax/AllSyntax.h"
#include "slang/syntax/SyntaxTree.h"
#include "slang/syntax/SyntaxVisitor.h"

namespace uvmcheck::rules::concurrency {

class BlockingIntfDriveVisitor : public engine::SyntaxVisitor<BlockingIntfDriveVisitor> {
public:
    explicit BlockingIntfDriveVisitor(core::DiagnosticEngine& diags) : diags(diags) {}

    static bool insideFunction(const slang::syntax::SyntaxNode& node) {
        for (const auto* p = node.parent.get(); p; p = p->parent.get()) {
            if (p->kind == slang::syntax::SyntaxKind::FunctionDeclaration) return true;
            if (p->kind == slang::syntax::SyntaxKind::TaskDeclaration) return false;
        }
        return false;
    }

    static bool insideContinuousAssign(const slang::syntax::SyntaxNode& node) {
        for (const auto* p = node.parent.get(); p; p = p->parent.get()) {
            if (p->kind == slang::syntax::SyntaxKind::ContinuousAssign) return true;
        }
        return false;
    }

    void handle(const slang::syntax::BinaryExpressionSyntax& expr) {
        // A continuous assignment drives the signal outside any process, so there is no
        // Active-region race to avoid, and 'assign x <= y' is not legal SystemVerilog.
        if (insideContinuousAssign(expr)) {
            visitDefault(expr);
            return;
        }
        if (expr.kind == slang::syntax::SyntaxKind::AssignmentExpression &&
            expr.operatorToken.kind == slang::parsing::TokenKind::Equals) {
            std::string lhs = std::string(expr.left->toString());
            lhs.erase(0, lhs.find_first_not_of(" \t\n\r"));
            if (size_t end = lhs.find_last_not_of(" \t\n\r"); end != std::string::npos) {
                lhs.erase(end + 1);
            }

            if (lhs.find(".cb.") != std::string::npos || lhs.starts_with("cb.") ||
                lhs.find("vif.") != std::string::npos || lhs.find("_vif.") != std::string::npos) {
                std::vector<core::FixItHint> fixIts;
                if (expr.operatorToken.range().start().valid()) {
                    // A function may not hold '<=' cleanly either (SV-LANG-005 would undo the
                    // fix), so there the rewrite is only shown: the drive belongs in a task.
                    const bool inFunction = insideFunction(expr);
                    fixIts.push_back({
                        expr.operatorToken.range(),
                        "<=",
                        inFunction ? "Move the drive into a task and use non-blocking '<='"
                                   : "Replace blocking assignment '=' with non-blocking clocking drive '<='",
                        inFunction ? core::FixSafety::DisplayOnly : core::FixSafety::Unsafe
                    });
                }

                diags.addViolation(
                    "SV-INT-001",
                    "BlockingAssignmentToInterface",
                    "Blocking assignment '=' used to drive clocking block or interface signal '" + lhs +
                    "'. In SystemVerilog (IEEE 1800 § 14.16), clocking block and interface signals must be driven using non-blocking clocking drive ('<=') to avoid simulation race conditions with DUT Active-region logic.",
                    core::Severity::Error,
                    expr.operatorToken.location(),
                    std::move(fixIts)
                );
            }
        }
        visitDefault(expr);
    }

private:
    core::DiagnosticEngine& diags;
};

class BlockingAssignmentToInterface : public Rule {
public:
    std::string_view getId() const override { return "SV-INT-001"; }
    std::string_view getName() const override { return "BlockingAssignmentToInterface"; }
    std::string_view getDescription() const override {
        return "Disallows blocking assignments ('=') when driving virtual interface or clocking block signals. "
               "Clocking blocks and synchronous interface signals must be driven with non-blocking drives ('<=') per IEEE 1800 § 14.16.";
    }
    std::string_view getCategory() const override { return "Concurrency"; }
    core::Severity getDefaultSeverity() const override { return core::Severity::Error; }

    engine::RuleTask check(engine::RuleContext& ctx) override {
        auto& diags = ctx.diags();
        BlockingIntfDriveVisitor visitor(diags);
        co_await ctx.visitSyntax(visitor);
    }
};

} // namespace uvmcheck::rules::concurrency

namespace uvmcheck::rules {
std::unique_ptr<Rule> createBlockingAssignmentToInterface() {
    return std::make_unique<concurrency::BlockingAssignmentToInterface>();
}
}

UVMCHECK_REGISTER_RULE(uvmcheck::rules::createBlockingAssignmentToInterface)
