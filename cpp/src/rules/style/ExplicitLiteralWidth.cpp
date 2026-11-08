#include "uvmcheck/rules/Rule.h"
#include "uvmcheck/rules/RuleRegistry.h"
#include "slang/ast/ASTVisitor.h"
#include "slang/ast/expressions/LiteralExpressions.h"
#include "slang/ast/symbols/MemberSymbols.h"
#include "slang/ast/symbols/ParameterSymbols.h"
#include "slang/syntax/SyntaxPrinter.h"
#include "slang/syntax/SyntaxVisitor.h"
#include "slang/syntax/AllSyntax.h"
#include <algorithm>
#include <cctype>

namespace uvmcheck::rules::style {

class ExplicitLiteralWidthVisitor : public engine::AstVisitor<ExplicitLiteralWidthVisitor> {
public:
    /// A literal's text: its tokens, without whitespace or comments.
    static std::string tokenText(const slang::syntax::SyntaxNode& n) {
        std::string text = slang::syntax::SyntaxPrinter().setIncludeTrivia(false).print(n).str();
        text.erase(std::remove_if(text.begin(), text.end(), [](unsigned char c) { return std::isspace(c); }), text.end());
        return text;
    }

    ExplicitLiteralWidthVisitor(core::DiagnosticEngine& diags, const slang::SourceManager& sm)
        : diags(diags), sm(sm) {}

    // `__LINE__ (as in `uvm_error's `uvm_line): a number the preprocessor
    // wrote, not a literal anyone can size.
    bool fromLineMacro(slang::SourceLocation loc) const {
        while (loc.valid() && sm.isMacroLoc(loc)) loc = sm.getOriginalLoc(loc);
        if (!loc.valid()) return false;
        auto text = sm.getSourceText(loc.buffer());
        return loc.offset() < text.size() && text.substr(loc.offset()).starts_with("`__LINE__");
    }

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

    /// The unsized literals written in a parameter's default value, as the
    /// AST would hold them: a literal's outer parentheses are its syntax, and
    /// a cast's width or type is not an operand.
    struct DefaultLiterals : slang::syntax::SyntaxVisitor<DefaultLiterals> {
        // (syntax whose text is printed, the literal itself)
        std::vector<std::pair<const slang::syntax::SyntaxNode*, const slang::syntax::SyntaxNode*>> found;

        static bool isLiteral(const slang::syntax::SyntaxNode& n) {
            using slang::syntax::SyntaxKind;
            return n.kind == SyntaxKind::IntegerLiteralExpression ||
                   (n.kind == SyntaxKind::IntegerVectorExpression &&
                    !n.as<slang::syntax::IntegerVectorExpressionSyntax>().size);
        }
        void handle(const slang::syntax::ParenthesizedExpressionSyntax& p) {
            const slang::syntax::ExpressionSyntax* e = p.expression;
            while (e->kind == slang::syntax::SyntaxKind::ParenthesizedExpression)
                e = e->as<slang::syntax::ParenthesizedExpressionSyntax>().expression;
            if (isLiteral(*e)) found.emplace_back(&p, e);
            else visitDefault(p);
        }
        void handle(const slang::syntax::LiteralExpressionSyntax& l) {
            if (isLiteral(l)) found.emplace_back(&l, &l);
        }
        void handle(const slang::syntax::IntegerVectorExpressionSyntax& v) {
            if (isLiteral(v)) found.emplace_back(&v, &v);
        }
        void handle(const slang::syntax::CastExpressionSyntax& c) { c.right->visit(*this); }
    };

    // An overridden parameter's default is never bound, so the AST does not
    // hold its literals; they are still written code. Without this a default
    // is reported or not depending on which specialization slang happened to
    // create first (an explicit #(32) taking the place of the default one).
    void handle(const slang::ast::ParameterSymbol& p) {
        const auto* syn = p.getSyntax();
        if (p.isOverridden() && syn && syn->kind == slang::syntax::SyntaxKind::Declarator) {
            const auto& d = syn->as<slang::syntax::DeclaratorSyntax>();
            if (d.initializer) {
                reportWritten(*d.initializer->expr);
            }
        }
        visitDefault(p);
    }

    /// Reports the unsized literals written in `syntax` (an expression or a
    /// task's arguments) that the AST does not hold.
    void reportWritten(const slang::syntax::SyntaxNode& syntax) {
        DefaultLiterals lits;
        syntax.visit(lits);
        for (const auto& [outer, inner] : lits.found) {
            auto loc = inner->getFirstToken().location();
            if (fromLineMacro(loc)) continue;
            report(tokenText(*outer), decimalAboveOne(*inner), loc, {});
        }
    }

    // In a generate block no instance takes, slang drops an assignment whose
    // selects are out of range for these parameters (`c[63:W]` with W = 64)
    // and never binds an elaboration task's arguments. That code is live under
    // other parameters, so its literals are reported as written.
    void handle(const slang::ast::ContinuousAssignSymbol& a) {
        if (const auto* syn = a.getSyntax(); syn && a.getParentScope() &&
            a.getParentScope()->isUninstantiated() && a.getAssignment().bad()) {
            reportWritten(*syn);
        }
        visitDefault(a);
    }

    void handle(const slang::ast::ElabSystemTaskSymbol& t) {
        if (const auto* syn = t.getSyntax(); syn && t.getParentScope() && t.getParentScope()->isUninstantiated()) {
            if (const auto* args = syn->as<slang::syntax::ElabSystemTaskSyntax>().arguments) reportWritten(*args);
        }
        visitDefault(t);
    }

    static bool decimalAboveOne(const slang::syntax::SyntaxNode& lit) {
        if (lit.kind != slang::syntax::SyntaxKind::IntegerLiteralExpression) return false;
        auto v = lit.as<slang::syntax::LiteralExpressionSyntax>().literal.intValue();
        return static_cast<bool>(v > 1);
    }

    void report(const std::string& text, bool aboveOne, slang::SourceLocation loc,
                std::vector<core::FixItHint> fix) {
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
                loc,
                std::move(fix)
            );
        } else if (text.find('\'') == std::string::npos && aboveOne) {
            // Plain decimal number without base specifier (e.g. 1024)
            // Filter out 0 and 1 which are standard in array bounds and loops
            diags.addViolation(
                "SV-STY-010",
                "ExplicitLiteralWidth",
                "Unsized decimal literal '" + text + "' lacks explicit bit-width specification. "
                "Specify explicit bit-width (e.g. 32'd" + text + ") to guarantee deterministic signedness and width sizing.",
                core::Severity::Info,
                loc
            );
        }
    }

    void handle(const slang::ast::IntegerLiteral& lit) {
        if (lit.isDeclaredUnsized && lit.syntax && !fromLineMacro(lit.sourceRange.start())) {
            std::string text = tokenText(*lit.syntax);
            report(text, static_cast<bool>(lit.getValue() > 1), lit.sourceRange.start(), widthFix(lit, text));
        }
        visitDefault(lit);
    }

private:
    core::DiagnosticEngine& diags;
    const slang::SourceManager& sm;
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
        ExplicitLiteralWidthVisitor visitor(diags, *ctx.compilation().getSourceManager());
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
