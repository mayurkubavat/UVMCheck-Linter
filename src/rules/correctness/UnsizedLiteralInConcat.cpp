#include "uvmcheck/rules/Rule.h"
#include "uvmcheck/rules/RuleRegistry.h"
#include "slang/ast/ASTVisitor.h"
#include "slang/ast/expressions/OperatorExpressions.h"
#include "slang/ast/expressions/LiteralExpressions.h"
#include "slang/syntax/AllSyntax.h"

namespace uvmcheck::rules::correctness {

class UnsizedLiteralInConcatVisitor : public slang::ast::ASTVisitor<UnsizedLiteralInConcatVisitor, slang::ast::VisitFlags::AllGood> {
public:
    explicit UnsizedLiteralInConcatVisitor(core::DiagnosticEngine& diags) : diags(diags) {}

    void handle(const slang::ast::ConcatenationExpression& expr) {
        for (const auto* operand : expr.operands()) {
            if (operand && operand->kind == slang::ast::ExpressionKind::IntegerLiteral) {
                const auto& lit = operand->as<slang::ast::IntegerLiteral>();
                if (lit.isDeclaredUnsized) {
                    std::string litText;
                    if (lit.syntax) {
                        litText = std::string(lit.syntax->toString());
                    }
                    // Check if it is an unbased decimal integer literal (e.g. 0, 1, 42)
                    if (!litText.empty() && litText.find('\'') == std::string::npos) {
                        std::string replacement;
                        if (lit.getValue() == 0) {
                            replacement = "1'b0";
                        } else if (lit.getValue() == 1) {
                            replacement = "1'b1";
                        } else {
                            replacement = "32'd" + litText;
                        }

                        std::vector<core::FixItHint> fixIts;
                        if (lit.sourceRange.start().valid()) {
                            fixIts.push_back({
                                lit.sourceRange,
                                replacement,
                                "Replace unsized literal with explicitly sized '" + replacement + "'"
                            });
                        }

                        diags.addViolation(
                            "SV-LANG-003",
                            "UnsizedLiteralInConcat",
                            "Unsized literal '" + litText + "' inside concatenation expands to 32 bits per IEEE 1800 § 11.4.12. "
                            "If a single bit was intended, specify explicit width (e.g. '" + replacement + "') "
                            "to prevent unintended 32-bit vector inflation and truncation.",
                            core::Severity::Warning,
                            lit.sourceRange.start(),
                            std::move(fixIts)
                        );
                    }
                }
            }
        }
        visitDefault(expr);
    }

private:
    core::DiagnosticEngine& diags;
};

class UnsizedLiteralInConcat : public Rule {
public:
    std::string_view getId() const override { return "SV-LANG-003"; }
    std::string_view getName() const override { return "UnsizedLiteralInConcat"; }
    std::string_view getDescription() const override {
        return "Disallow unbased unsized integer literals inside concatenations ({1, 0, data}). "
               "Per IEEE 1800 § 11.4.12, unbased constants expand to 32 bits each, causing unintended vector widening.";
    }
    std::string_view getCategory() const override { return "Correctness"; }
    core::Severity getDefaultSeverity() const override { return core::Severity::Warning; }

    void run(slang::ast::Compilation& compilation,
             const uvmcheck::uvm::UvmModel& /*uvmModel*/,
             core::DiagnosticEngine& diags) override {
        UnsizedLiteralInConcatVisitor visitor(diags);
        compilation.getRoot().visit(visitor);
    }
};

} // namespace uvmcheck::rules::correctness

namespace uvmcheck::rules {
std::unique_ptr<Rule> createUnsizedLiteralInConcat() {
    return std::make_unique<correctness::UnsizedLiteralInConcat>();
}
}
