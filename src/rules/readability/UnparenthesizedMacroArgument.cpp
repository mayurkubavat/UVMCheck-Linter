// SV-MAC-002: an expression macro that does not parenthesize its arguments or its body.

#include "uvmcheck/rules/Rule.h"
#include "uvmcheck/rules/RuleRegistry.h"

#include "slang/parsing/LexerFacts.h"
#include "slang/syntax/AllSyntax.h"
#include "slang/syntax/SyntaxTree.h"
#include "slang/text/SourceManager.h"

#include <algorithm>
#include <set>
#include <string>
#include <vector>

namespace uvmcheck::rules::readability {

using namespace slang::syntax;
using slang::parsing::Token;
using slang::parsing::TokenKind;

namespace {

bool isOperator(TokenKind k) {
    switch (k) {
        case TokenKind::Plus: case TokenKind::Minus: case TokenKind::Star: case TokenKind::Slash:
        case TokenKind::Percent: case TokenKind::DoubleStar: case TokenKind::LeftShift:
        case TokenKind::RightShift: case TokenKind::TripleLeftShift: case TokenKind::TripleRightShift:
        case TokenKind::And: case TokenKind::Or: case TokenKind::Xor: case TokenKind::DoubleAnd:
        case TokenKind::DoubleOr: case TokenKind::DoubleEquals: case TokenKind::ExclamationEquals:
        case TokenKind::TripleEquals: case TokenKind::ExclamationDoubleEquals: case TokenKind::LessThan:
        case TokenKind::LessThanEquals: case TokenKind::GreaterThan: case TokenKind::GreaterThanEquals:
        case TokenKind::Question: case TokenKind::Tilde: case TokenKind::Exclamation:
            return true;
        default:
            return false;
    }
}

bool isBinaryOperator(TokenKind k) {
    return isOperator(k) && k != TokenKind::Tilde && k != TokenKind::Exclamation;
}

/// True when the whole body is one parenthesized group: ( ... ).
bool wrappedInParens(const std::vector<Token>& body) {
    if (body.size() < 2 || body.front().kind != TokenKind::OpenParenthesis ||
        body.back().kind != TokenKind::CloseParenthesis)
        return false;
    int depth = 0;
    for (size_t i = 0; i < body.size(); ++i) {
        if (body[i].kind == TokenKind::OpenParenthesis) ++depth;
        else if (body[i].kind == TokenKind::CloseParenthesis && --depth == 0 && i + 1 != body.size()) return false;
    }
    return true;
}

/// Problems in one macro, or empty when it is fine (or not an expression macro).
std::string problems(const DefineDirectiveSyntax& def) {
    if (!def.formalArguments) return {};
    std::vector<Token> body(def.body.begin(), def.body.end());
    bool hasOperator = false;
    for (const auto& t : body) {
        // Statements, declarations and pasted or stringified text are not plain expressions.
        if (t.kind == TokenKind::Semicolon || slang::parsing::LexerFacts::isKeyword(t.kind) ||
            t.kind == TokenKind::MacroPaste || t.kind == TokenKind::MacroQuote ||
            t.kind == TokenKind::MacroTripleQuote || t.kind == TokenKind::Directive ||
            t.kind == TokenKind::MacroUsage)
            return {};
        hasOperator |= isOperator(t.kind);
    }
    if (!hasOperator) return {};

    std::set<std::string_view> formals;
    for (const auto* arg : def.formalArguments->args) formals.insert(arg->name.valueText());

    std::vector<std::string> bare;
    for (size_t i = 0; i < body.size(); ++i) {
        if (body[i].kind != TokenKind::Identifier || !formals.count(body[i].valueText())) continue;
        const bool before = i > 0 && isOperator(body[i - 1].kind);
        const bool after = i + 1 < body.size() && isBinaryOperator(body[i + 1].kind);
        if (before || after) {
            std::string name(body[i].valueText());
            if (std::find(bare.begin(), bare.end(), name) == bare.end()) bare.push_back(name);
        }
    }
    std::string out;
    if (!bare.empty()) {
        out = "argument";
        out += bare.size() > 1 ? "s " : " ";
        for (size_t i = 0; i < bare.size(); ++i) out += (i ? ", '" : "'") + bare[i] + "'";
        out += " used next to an operator without parentheses";
    }
    if (!wrappedInParens(body)) out += std::string(out.empty() ? "" : "; ") + "body not wrapped in parentheses";
    return out;
}

} // namespace

class UnparenthesizedMacroArgument : public Rule {
public:
    std::string_view getId() const override { return "SV-MAC-002"; }
    std::string_view getName() const override { return "UnparenthesizedMacroArgument"; }
    std::string_view getDescription() const override {
        return "Flags expression macros that use an argument next to an operator without parentheses, or "
               "whose body is not wrapped in parentheses. `define DOUBLE(x) x*2 turns DOUBLE(a+1) into "
               "a+1*2; write `define DOUBLE(x) ((x)*2).";
    }
    std::string_view getCategory() const override { return "Correctness"; }
    core::Severity getDefaultSeverity() const override { return core::Severity::Warning; }

    engine::RuleTask check(engine::RuleContext& ctx) override {
        co_await ctx.standaloneJob();
        auto& comp = ctx.compilation();
        const auto* sm = comp.getSourceManager();
        if (!sm) co_return;
        std::set<std::pair<uint32_t, size_t>> seen;
        for (const auto& tree : comp.getSyntaxTrees()) {
            if (!tree) continue;
            for (const auto* def : tree->getDefinedMacros()) {
                auto loc = def->name.location();
                if (!loc.valid() || !sm->isFileLoc(loc)) continue;
                auto name = def->name.valueText();
                if (name.starts_with("uvm_") || name.starts_with("UVM_")) continue;
                if (!seen.insert({loc.buffer().getId(), loc.offset()}).second) continue;
                std::string p = problems(*def);
                if (p.empty()) continue;
                ctx.diags().addViolation(
                    getId(), getName(),
                    "Macro '" + std::string(name) + "': " + p + ". After expansion, operator precedence "
                    "can regroup the caller's expression. Parenthesize each use of an argument and the whole "
                    "body: `define " + std::string(name) + "(...) ((a) op (b)).",
                    core::Severity::Warning, loc);
            }
        }
    }
};

} // namespace uvmcheck::rules::readability

namespace uvmcheck::rules {
std::unique_ptr<Rule> createUnparenthesizedMacroArgument() {
    return std::make_unique<readability::UnparenthesizedMacroArgument>();
}
} // namespace uvmcheck::rules

UVMCHECK_REGISTER_RULE(uvmcheck::rules::createUnparenthesizedMacroArgument)
