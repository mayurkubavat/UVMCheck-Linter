#include "uvmcheck/rules/Rule.h"
#include "uvmcheck/rules/RuleRegistry.h"
#include "slang/ast/ASTVisitor.h"
#include "slang/ast/expressions/CallExpression.h"
#include "slang/ast/expressions/ConversionExpression.h"
#include "slang/ast/symbols/ClassSymbols.h"
#include "slang/syntax/AllSyntax.h"
#include <string>

namespace uvmcheck::rules::uvm {

class PSequencerCastVisitor : public slang::ast::ASTVisitor<PSequencerCastVisitor, slang::ast::VisitFlags::AllGood> {
public:
    explicit PSequencerCastVisitor(core::DiagnosticEngine& diags) : diags(diags) {}

    void handle(const slang::ast::ExpressionStatement& stmt) {
        checkExpr(stmt.expr, stmt.sourceRange);
        visitDefault(stmt);
    }

private:
    core::DiagnosticEngine& diags;

    void checkExpr(const slang::ast::Expression& expr, slang::SourceRange range) {
        if (expr.kind == slang::ast::ExpressionKind::Call) {
            checkCall(expr.as<slang::ast::CallExpression>(), range);
        } else if (expr.kind == slang::ast::ExpressionKind::Conversion) {
            const auto& conv = expr.as<slang::ast::ConversionExpression>();
            if (conv.type->isVoid() && conv.operand().kind == slang::ast::ExpressionKind::Call) {
                checkCall(conv.operand().as<slang::ast::CallExpression>(), range);
            }
        }
    }

    void checkCall(const slang::ast::CallExpression& call, slang::SourceRange range) {
        if (call.getSubroutineName() == "$cast") {
            auto args = call.arguments();
            if (args.size() >= 2 && args[0] && args[1]) {
                std::string destName = "";
                if (args[0]->syntax) destName = std::string(args[0]->syntax->toString());
                std::string srcName = "";
                if (args[1]->syntax) srcName = std::string(args[1]->syntax->toString());

                bool isPSeqCast = (destName.find("p_sequencer") != std::string::npos ||
                                   srcName.find("m_sequencer") != std::string::npos);

                if (!isPSeqCast && call.syntax) {
                    std::string callStr = std::string(call.syntax->toString());
                    if (callStr.find("p_sequencer") != std::string::npos ||
                        callStr.find("m_sequencer") != std::string::npos) {
                        isPSeqCast = true;
                    }
                }

                if (isPSeqCast) {
                    diags.addViolation(
                        "UVM-SEQ-011",
                        "UncheckedPSequencerCast",
                        "Dynamic cast '$cast(" + (destName.empty() ? "p_sequencer" : destName) + ", " +
                        (srcName.empty() ? "m_sequencer" : srcName) + ")' is executed without checking its return status. "
                        "In UVM sequence architecture (IEEE 1800.2 § 5.8), if the sequence is started on an "
                        "incompatible sequencer or virtual sequencer, an unchecked $cast leaves 'p_sequencer' null, resulting in an immediate "
                        "null-pointer simulation crash. Always check the return value (e.g. 'if (!$cast(p_sequencer, m_sequencer)) `uvm_fatal(...)').",
                        core::Severity::Warning,
                        range.start()
                    );
                }
            }
        }
    }
};

class UncheckedPSequencerCast : public Rule {
public:
    std::string_view getId() const override { return "UVM-SEQ-011"; }
    std::string_view getName() const override { return "UncheckedPSequencerCast"; }
    std::string_view getDescription() const override {
        return "Flags unchecked $cast(p_sequencer, m_sequencer) calls in sequences that do not verify the return status.";
    }
    std::string_view getCategory() const override { return "UVM"; }
    core::Severity getDefaultSeverity() const override { return core::Severity::Warning; }

    void run(slang::ast::Compilation& compilation,
             const uvmcheck::uvm::UvmModel& /*uvmModel*/,
             core::DiagnosticEngine& diags) override {
        PSequencerCastVisitor visitor(diags);
        compilation.getRoot().visit(visitor);
    }
};

} // namespace uvmcheck::rules::uvm

namespace uvmcheck::rules {
std::unique_ptr<Rule> createUncheckedPSequencerCast() {
    return std::make_unique<uvm::UncheckedPSequencerCast>();
}
} // namespace uvmcheck::rules
