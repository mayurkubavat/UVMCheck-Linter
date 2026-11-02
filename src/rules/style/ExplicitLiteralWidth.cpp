#include "uvmcheck/rules/Rule.h"
#include "uvmcheck/rules/RuleRegistry.h"
#include "slang/ast/ASTVisitor.h"
#include "slang/ast/expressions/LiteralExpressions.h"
#include "slang/syntax/AllSyntax.h"
#include <algorithm>
#include <cctype>

namespace uvmcheck::rules::style {

class ExplicitLiteralWidthVisitor : public engine::AstVisitor<ExplicitLiteralWidthVisitor> {
public:
    explicit ExplicitLiteralWidthVisitor(core::DiagnosticEngine& diags) : diags(diags) {}

    // 'hFF and 32'hFF are the same value with the same signedness, so writing the width the
    // literal already has is safe. Not for x/z digits: an unsized 'x fills any width it is
    // assigned to, a sized one only its own bits.
    static std::vector<core::FixItHint> widthFix(const slang::ast::IntegerLiteral& lit, const std::string& text) {
        if (text.find_first_of("xXzZ?", 2) != std::string::npos || !lit.type) return {};
        const auto width = lit.type->getBitWidth();
        if (width == 0) return {};
        return {{lit.sourceRange, std::to_string(width) + text,
                 "Write the width: " + std::to_string(width) + text}};
    }

    void handle(const slang::ast::IntegerLiteral& lit) {
        if (lit.isDeclaredUnsized && lit.syntax) {
            std::string text = lit.syntax->toString();
            text.erase(std::remove_if(text.begin(), text.end(), [](unsigned char c) { return std::isspace(c); }), text.end());

            // Check based unsized literals like 'hFF, 'b01, 'o77, 'd10
            if (text.starts_with("'h") || text.starts_with("'H") ||
                text.starts_with("'b") || text.starts_with("'B") ||
                text.starts_with("'o") || text.starts_with("'O") ||
                text.starts_with("'d") || text.starts_with("'D")) {
                diags.addViolation(
                    "SV-STY-010",
                    "ExplicitLiteralWidth",
                    "Numeric literal '" + text + "' is missing an explicit bit-width specification. "
                    "By convention, all numeric literals must declare explicit width (e.g. 8'hFF instead of 'hFF) to avoid 32-bit truncation and sign extension traps.",
                    core::Severity::Warning,
                    lit.sourceRange.start(),
                    widthFix(lit, text)
                );
            } else if (text.find('\'') == std::string::npos) {
                // Plain decimal number without base specifier (e.g. 1024)
                // Filter out 0 and 1 which are standard in array bounds and loops
                if (lit.getValue() > 1) {
                    diags.addViolation(
                        "SV-STY-010",
                        "ExplicitLiteralWidth",
                        "Unsized decimal literal '" + text + "' lacks explicit bit-width specification. "
                        "Specify explicit bit-width (e.g. 32'd" + text + ") to guarantee deterministic signedness and width sizing.",
                        core::Severity::Info,
                        lit.sourceRange.start()
                    );
                }
            }
        }
        visitDefault(lit);
    }

private:
    core::DiagnosticEngine& diags;
};

class ExplicitLiteralWidth : public Rule {
public:
    std::string_view getId() const override { return "SV-STY-010"; }
    std::string_view getName() const override { return "ExplicitLiteralWidth"; }
    std::string_view getDescription() const override {
        return "Disallow unsized integer and numeric literals without explicit bit-width declarations. "
               "Prevents silent 32-bit zero-extension and sign extension bugs.";
    }
    std::string_view getCategory() const override { return "Style"; }
    core::Severity getDefaultSeverity() const override { return core::Severity::Warning; }

    engine::RuleTask check(engine::RuleContext& ctx) override {
        auto& diags = ctx.diags();
        ExplicitLiteralWidthVisitor visitor(diags);
        co_await ctx.visitAst(visitor);
    }
};

} // namespace uvmcheck::rules::style

namespace uvmcheck::rules {
std::unique_ptr<Rule> createExplicitLiteralWidth() {
    return std::make_unique<style::ExplicitLiteralWidth>();
}
}

UVMCHECK_REGISTER_RULE(uvmcheck::rules::createExplicitLiteralWidth)
