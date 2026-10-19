#include "uvmcheck/rules/Rule.h"
#include "uvmcheck/rules/RuleRegistry.h"
#include "slang/syntax/AllSyntax.h"
#include "slang/syntax/SyntaxTree.h"
#include "slang/syntax/SyntaxVisitor.h"

namespace uvmcheck::rules::concurrency {

class BlockingIntfDriveVisitor : public slang::syntax::SyntaxVisitor<BlockingIntfDriveVisitor> {
public:
    explicit BlockingIntfDriveVisitor(core::DiagnosticEngine& diags) : diags(diags) {}

    void handle(const slang::syntax::BinaryExpressionSyntax& expr) {
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
                    fixIts.push_back({
                        expr.operatorToken.range(),
                        "<=",
                        "Replace blocking assignment '=' with non-blocking clocking drive '<='"
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

    void run(slang::ast::Compilation& compilation,
             const uvmcheck::uvm::UvmModel& /*uvmModel*/,
             core::DiagnosticEngine& diags) override {
        BlockingIntfDriveVisitor visitor(diags);
        for (const auto& tree : compilation.getSyntaxTrees()) {
            if (tree) {
                tree->root().visit(visitor);
            }
        }
    }
};

} // namespace uvmcheck::rules::concurrency

namespace uvmcheck::rules {
std::unique_ptr<Rule> createBlockingAssignmentToInterface() {
    return std::make_unique<concurrency::BlockingAssignmentToInterface>();
}
}
