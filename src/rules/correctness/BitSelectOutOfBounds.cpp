#include "uvmcheck/rules/Rule.h"
#include "uvmcheck/rules/RuleRegistry.h"
#include "slang/ast/ASTVisitor.h"
#include "slang/ast/expressions/SelectExpressions.h"
#include "slang/ast/expressions/LiteralExpressions.h"
#include "slang/ast/expressions/MiscExpressions.h"
#include "slang/syntax/AllSyntax.h"
#include <algorithm>
#include <cctype>

namespace uvmcheck::rules::correctness {

class BitSelectOutOfBoundsVisitor : public engine::AstVisitor<BitSelectOutOfBoundsVisitor> {
public:
    explicit BitSelectOutOfBoundsVisitor(core::DiagnosticEngine& diags) : diags(diags) {}

    void handle(const slang::ast::ElementSelectExpression& expr) {
        const auto& targetVal = expr.value();
        const auto& targetType = *targetVal.type;

        if (targetType.hasFixedRange()) {
            auto range = targetType.getFixedRange();

            if (expr.selector().kind == slang::ast::ExpressionKind::IntegerLiteral) {
                const auto& lit = expr.selector().as<slang::ast::IntegerLiteral>();
                if (!lit.getValue().hasUnknown()) {
                    auto valOpt = lit.getValue().as<int64_t>();
                    if (valOpt) {
                        int64_t idx = *valOpt;
                        if (idx < range.lower() || idx > range.upper()) {
                            std::string targetName;
                            if (targetVal.kind == slang::ast::ExpressionKind::NamedValue) {
                                targetName = std::string(targetVal.as<slang::ast::NamedValueExpression>().symbol.name);
                            } else if (targetVal.kind == slang::ast::ExpressionKind::MemberAccess) {
                                targetName = std::string(targetVal.as<slang::ast::MemberAccessExpression>().member.name);
                            } else if (targetVal.syntax) {
                                targetName = std::string(targetVal.syntax->toString());
                                targetName.erase(
                                    std::remove_if(targetName.begin(), targetName.end(),
                                                   [](unsigned char c) { return std::isspace(c); }),
                                    targetName.end()
                                );
                            }
                            if (targetName.empty()) {
                                targetName = "vector";
                            }

                            diags.addViolation(
                                "SV-LANG-002",
                                "BitSelectOutOfBounds",
                                "Static out-of-bounds bit/element select: index [" + std::to_string(idx) +
                                "] is outside declared range [" + std::to_string(range.left) + ":" +
                                std::to_string(range.right) + "] of '" + targetName + "'. "
                                "In SystemVerilog (IEEE 1800 § 7.4.5), reading out-of-bounds returns 'x "
                                "without crashing, silently propagating uninitialized state into downstream verification logic.",
                                core::Severity::Error,
                                expr.sourceRange.start()
                            );
                        }
                    }
                }
            }
        }
        visitDefault(expr);
    }

private:
    core::DiagnosticEngine& diags;
};

class BitSelectOutOfBounds : public Rule {
public:
    std::string_view getId() const override { return "SV-LANG-002"; }
    std::string_view getName() const override { return "BitSelectOutOfBounds"; }
    std::string_view getDescription() const override {
        return "Detects static out-of-bounds bit or element selects on vectors and arrays with fixed ranges. "
               "SystemVerilog silently returns 'x for out-of-bounds accesses without crashing, causing silent data corruption.";
    }
    std::string_view getCategory() const override { return "Correctness"; }
    core::Severity getDefaultSeverity() const override { return core::Severity::Error; }

    engine::RuleTask check(engine::RuleContext& ctx) override {
        auto& diags = ctx.diags();
        BitSelectOutOfBoundsVisitor visitor(diags);
        co_await ctx.visitAst(visitor);
    }
};

} // namespace uvmcheck::rules::correctness

namespace uvmcheck::rules {
std::unique_ptr<Rule> createBitSelectOutOfBounds() {
    return std::make_unique<correctness::BitSelectOutOfBounds>();
}
}
