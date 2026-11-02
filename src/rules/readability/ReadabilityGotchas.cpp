#include "uvmcheck/rules/Rule.h"
#include "uvmcheck/rules/RuleRegistry.h"
#include "uvmcheck/rules/SweSupport.h"
#include "slang/ast/ASTVisitor.h"
#include "slang/ast/expressions/OperatorExpressions.h"
#include "slang/ast/symbols/ClassSymbols.h"
#include "slang/ast/symbols/SubroutineSymbols.h"
#include "slang/ast/symbols/VariableSymbols.h"
#include "slang/parsing/Token.h"
#include "slang/syntax/AllSyntax.h"
#include "slang/syntax/SyntaxTree.h"
#include "slang/syntax/SyntaxVisitor.h"
#include "slang/text/SourceManager.h"
#include <algorithm>
#include <cctype>
#include <cstring>
#include <filesystem>
#include <set>
#include <string>
#include <vector>

namespace uvmcheck::rules::readability {

using namespace slang::ast;
using slang::parsing::Token;
using slang::parsing::TriviaKind;

namespace {

std::string lower(std::string_view s) {
    std::string out(s);
    std::transform(out.begin(), out.end(), out.begin(), [](unsigned char c) { return std::tolower(c); });
    return out;
}

// Words of an identifier: split on '_' and on lower-to-upper case changes, digits dropped.
std::vector<std::string> identWords(std::string_view name) {
    std::vector<std::string> out;
    std::string cur;
    auto flush = [&] {
        if (!cur.empty()) out.push_back(lower(cur));
        cur.clear();
    };
    for (size_t i = 0; i < name.size(); ++i) {
        const char c = name[i];
        if (c == '_' || std::isdigit(static_cast<unsigned char>(c))) {
            flush();
            continue;
        }
        if (std::isupper(static_cast<unsigned char>(c)) && !cur.empty() &&
            std::islower(static_cast<unsigned char>(cur.back()))) {
            flush();
        }
        cur += c;
    }
    flush();
    return out;
}

// Files of the UVM library and the test stub are not the user's to rename or comment.
bool isLibraryFile(const slang::SourceManager& sm, slang::SourceLocation loc) {
    if (!loc.valid() || sm.isMacroLoc(loc)) return true;
    const auto file = std::filesystem::path(std::string(sm.getFileName(loc))).filename().string();
    return file.starts_with("uvm_");
}

// User variables that a routine or class declares (not arguments, not compiler temporaries).
bool isUserVariable(const Symbol& s) {
    if (s.kind == SymbolKind::ClassProperty) return true;
    if (s.kind != SymbolKind::Variable) return false;
    const auto& v = s.as<VariableSymbol>();
    return !v.flags.has(VariableFlags::CompilerGenerated);
}

// Every token of a syntax node, in order.
std::vector<Token> tokensOf(const slang::syntax::SyntaxNode& node) {
    struct Collector : public slang::syntax::SyntaxVisitor<Collector> {
        std::vector<Token> tokens;
        void visitToken(Token t) { tokens.push_back(t); }
    } collector;
    node.visit(collector);
    return collector.tokens;
}

} // namespace

// --- SV-RDB-006: a negated negative flag ---------------------------------------------

bool isNegativeName(std::string_view name) {
    static const std::set<std::string> negatives = {"disable", "disabled", "dis", "no", "not", "inhibit",
                                                    "skip", "ignore", "without", "non", "suppress"};
    for (const auto& w : identWords(name)) {
        if (negatives.count(w)) return true;
    }
    return false;
}

class DoubleNegativeVisitor : public swe::UserCodeVisitor<DoubleNegativeVisitor> {
public:
    using UserCodeVisitor::handle;
    explicit DoubleNegativeVisitor(core::DiagnosticEngine& diags) : diags(diags) {}

    void handle(const UnaryExpression& u) {
        const bool negates = u.op == UnaryOperator::LogicalNot ||
                             (u.op == UnaryOperator::BitwiseNot && u.operand().type &&
                              u.operand().type->getBitWidth() == 1);
        if (negates) {
            if (const Symbol* s = swe::referencedSymbol(u.operand()); s && isUserVariable(*s) &&
                                                                     isNegativeName(s->name)) {
                diags.addViolation(
                    "SV-RDB-006", "DoubleNegativeFlag",
                    "'" + swe::exprText(u) + "' negates a negative flag ('" + std::string(s->name) +
                        "'), which readers have to invert twice. Name the flag for the positive case "
                        "(en_*, has_*, use_*) so the test reads straight.",
                    core::Severity::Info, u.sourceRange.start());
            }
        }
        visitDefault(u);
    }

private:
    core::DiagnosticEngine& diags;
};

class DoubleNegativeFlag : public Rule {
public:
    std::string_view getId() const override { return "SV-RDB-006"; }
    std::string_view getName() const override { return "DoubleNegativeFlag"; }
    std::string_view getDescription() const override {
        return "Flags '!' (or '~' on one bit) applied to a variable whose name is negative "
               "(disable_*, no_*, not_*, skip_*, inhibit_*, ...). Advisory: off by default.";
    }
    std::string_view getCategory() const override { return "Advisory"; }
    core::Severity getDefaultSeverity() const override { return core::Severity::Info; }

    engine::RuleTask check(engine::RuleContext& ctx) override {
        DoubleNegativeVisitor visitor(ctx.diags());
        co_await ctx.visitAst(visitor);
    }
};

// --- SV-RDB-007: names that say nothing ----------------------------------------------

// `data` and `value` are fine as class fields (a bus item's data beat, a register's value);
// as locals they usually stand in for a real name.
bool isGenericName(std::string_view name, bool local) {
    static const std::set<std::string> stems = {"tmp", "temp", "val", "obj", "foo", "bar", "baz", "var",
                                                "thing", "stuff", "dummy", "d", "t"};
    std::string n = lower(name);
    while (n.size() > 1 && std::isdigit(static_cast<unsigned char>(n.back()))) n.pop_back();
    return stems.count(n) != 0 || (local && (n == "data" || n == "value"));
}

class NonDescriptiveName : public Rule {
public:
    std::string_view getId() const override { return "SV-RDB-007"; }
    std::string_view getName() const override { return "NonDescriptiveName"; }
    std::string_view getDescription() const override {
        return "Flags class properties, and locals of routines longer than min_lines (default 10), "
               "named tmp, temp, val, obj, foo, d, t and the like (with or without a number; locals also "
               "data and value). "
               "Arguments and loop variables are not checked. Advisory: off by default.";
    }
    std::string_view getCategory() const override { return "Advisory"; }
    core::Severity getDefaultSeverity() const override { return core::Severity::Info; }

    void run(slang::ast::Compilation& compilation, const uvm::UvmModel& model, core::DiagnosticEngine& diags) override {
        const auto minLines = swe::ruleOption("SV-RDB-007", "min_lines", 10);
        auto report = [&](const Symbol& s, std::string_view where) {
            diags.addViolation(
                "SV-RDB-007", "NonDescriptiveName",
                "'" + std::string(s.name) + "' " + std::string(where) +
                    " says nothing about what it holds. Name it for its meaning (for example "
                    "aligned_addr, expected_txn) so logs and waveforms read without the source.",
                core::Severity::Info, s.location);
        };
        for (const auto* cls : swe::userClasses(model)) {
            for (const auto& prop : cls->membersOfType<ClassPropertySymbol>()) {
                if (isGenericName(prop.name, false)) report(prop, "in class '" + std::string(cls->name) + "'");
            }
            for (const auto* method : swe::classMethods(*cls)) {
                if (method->getParentScope() != cls) continue;
                const auto range = method->getBody().sourceRange;
                if (!range.start().valid() ||
                    static_cast<int64_t>(swe::lineOf(compilation, range.end()) -
                                         swe::lineOf(compilation, range.start())) < minLines) {
                    continue;
                }
                LocalCollector locals;
                method->visit(locals);
                for (const auto* v : locals.vars) {
                    if (isGenericName(v->name, true)) report(*v, "in '" + std::string(method->name) + "'");
                }
            }
        }
    }

private:
    struct LocalCollector : public ASTVisitor<LocalCollector, VisitFlags::AllGood> {
        std::vector<const VariableSymbol*> vars;
        void handle(const VariableSymbol& v) {
            if (v.kind == SymbolKind::Variable && !v.flags.has(VariableFlags::CompilerGenerated)) vars.push_back(&v);
        }
        void handle(const FormalArgumentSymbol&) {}
        void handle(const ForLoopStatement& loop) {
            // Loop counters (for (int i ...)) are named by convention; skip them, keep the body.
            loop.body.visit(*this);
        }
    };
};

// --- SV-RDB-008: vowel-stripped abbreviations ----------------------------------------

bool isUnclearAbbreviation(const std::string& word) {
    static const std::set<std::string> lexicon = {
        "clk", "rst", "cfg", "cmd", "pkt", "txn", "req", "rsp", "vif", "drv", "mon", "sqr", "scb", "env",
        "agt", "ral", "csr", "irq", "crc", "fsm", "spi", "ddr", "dll", "pll", "phy", "rtl", "hdl", "dpm",
        "xfr", "msg", "ptr", "cnt", "len", "buf", "src", "dst", "mst", "slv", "lst", "pwr", "mmcm",
        "bfm", "dmn", "dsp", "jtag", "gpr", "rcv", "trn", "sts", "ctl", "ctrl", "ctx", "hw", "sw"};
    if (word.size() < 4) return false;
    for (char c : word) {
        if (std::strchr("aeiouy", c)) return false;
    }
    if (lexicon.count(word)) return false;
    // Plurals of known short forms (pkts, txns) read fine.
    if (word.back() == 's' && lexicon.count(word.substr(0, word.size() - 1))) return false;
    return true;
}

class UnclearAbbreviation : public Rule {
public:
    std::string_view getId() const override { return "SV-RDB-008"; }
    std::string_view getName() const override { return "UnclearAbbreviation"; }
    std::string_view getDescription() const override {
        return "Flags class, property, method and local names containing a vowel-less word of four "
               "or more letters (trnsfr, chnnl, rgstr) that is not a common DV short form (clk, cfg, "
               "ctrl, jtag, ...). Each person drops different vowels, so such names defeat grep. "
               "Advisory: off by default.";
    }
    std::string_view getCategory() const override { return "Advisory"; }
    core::Severity getDefaultSeverity() const override { return core::Severity::Info; }

    void run(slang::ast::Compilation&, const uvm::UvmModel& model, core::DiagnosticEngine& diags) override {
        std::set<const Symbol*> seen;
        auto check = [&](const Symbol& s) {
            if (!seen.insert(&s).second) return;
            for (const auto& w : identWords(s.name)) {
                if (!isUnclearAbbreviation(w)) continue;
                diags.addViolation(
                    "SV-RDB-008", "UnclearAbbreviation",
                    "'" + std::string(s.name) + "' abbreviates by dropping vowels ('" + w +
                        "'). Spell the word out, or use the team's agreed short form.",
                    core::Severity::Info, s.location);
                return;
            }
        };
        for (const auto* cls : swe::userClasses(model)) {
            check(*cls);
            for (const auto& prop : cls->membersOfType<ClassPropertySymbol>()) check(prop);
            for (const auto* method : swe::classMethods(*cls)) {
                if (method->getParentScope() != cls) continue;
                check(*method);
                for (const auto* arg : method->getArguments()) check(*arg);
            }
        }
    }
};

// --- SV-RDB-009: a comment that repeats the statement under it ------------------------

class RedundantCommentScanner : public engine::SyntaxVisitor<RedundantCommentScanner> {
public:
    RedundantCommentScanner(const slang::SourceManager& sm, core::DiagnosticEngine& diags) : sm(sm), diags(diags) {}

    void handle(const slang::syntax::ExpressionStatementSyntax& stmt) {
        check(stmt);
        visitDefault(stmt);
    }

private:
    void check(const slang::syntax::SyntaxNode& stmt) {
        const Token first = stmt.getFirstToken();
        if (isLibraryFile(sm, first.location())) return;

        // The comment must sit alone on the line right above the statement.
        std::string_view comment;
        int eolsAfter = 0;
        for (const auto& t : first.trivia()) {
            if (t.kind == TriviaKind::LineComment) {
                comment = t.getRawText();
                eolsAfter = 0;
            } else if (t.kind == TriviaKind::EndOfLine) {
                ++eolsAfter;
            } else if (t.kind != TriviaKind::Whitespace) {
                comment = {};
            }
        }
        if (comment.empty() || eolsAfter != 1) return;

        static const std::set<std::string> stop = {"the", "a", "an", "to", "of", "and", "for", "in", "on",
                                                   "with", "this", "that", "is", "it", "from", "into"};
        std::vector<std::string> words;
        std::string cur;
        for (char c : comment.substr(2)) {
            if (std::isalnum(static_cast<unsigned char>(c))) {
                cur += static_cast<char>(std::tolower(static_cast<unsigned char>(c)));
            } else {
                if (!cur.empty() && !stop.count(cur)) words.push_back(cur);
                cur.clear();
            }
        }
        if (!cur.empty() && !stop.count(cur)) words.push_back(cur);
        if (words.size() < 2 || words.size() > 6) return;

        std::set<std::string> code;
        for (const Token& tok : tokensOf(stmt)) {
            if (tok.kind != slang::parsing::TokenKind::Identifier) continue;
            for (const auto& w : identWords(tok.valueText())) code.insert(w);
        }
        auto covered = [&](const std::string& w) {
            for (const auto& c : code) {
                if (c == w || (c.size() >= 3 && w.size() >= 3 && (c.starts_with(w) || w.starts_with(c)))) return true;
            }
            return false;
        };
        if (!std::all_of(words.begin(), words.end(), covered)) return;

        // Point at the comment: trivia text lives in the same buffer as the token.
        const auto tokLoc = first.location();
        const auto delta = static_cast<uint64_t>(first.rawText().data() - comment.data());
        const slang::SourceLocation loc =
            tokLoc.offset() >= delta ? slang::SourceLocation(tokLoc.buffer(), tokLoc.offset() - delta) : tokLoc;
        diags.addViolation(
            "SV-RDB-009", "RedundantComment",
            "The comment only repeats the code below it. Say why the line is there (the protocol "
            "rule, the workaround, the corner case), or remove the comment.",
            core::Severity::Info, loc);
    }

    const slang::SourceManager& sm;
    core::DiagnosticEngine& diags;
};

class RedundantComment : public Rule {
public:
    std::string_view getId() const override { return "SV-RDB-009"; }
    std::string_view getName() const override { return "RedundantComment"; }
    std::string_view getDescription() const override {
        return "Flags a one-line // comment directly above a statement when every word of the comment "
               "(2 to 6 words, ignoring a, the, to, ...) is already a word of the statement's names, "
               "e.g. '// set cfg' above set_cfg(). Advisory: off by default.";
    }
    std::string_view getCategory() const override { return "Advisory"; }
    core::Severity getDefaultSeverity() const override { return core::Severity::Info; }

    engine::RuleTask check(engine::RuleContext& ctx) override {
        RedundantCommentScanner scanner(*ctx.compilation().getSourceManager(), ctx.diags());
        co_await ctx.visitSyntax(scanner);
    }
};

// --- SV-RDB-010: components and sequences without a description ----------------------

class UndocumentedTestbenchClass : public Rule {
public:
    std::string_view getId() const override { return "SV-RDB-010"; }
    std::string_view getName() const override { return "UndocumentedTestbenchClass"; }
    std::string_view getDescription() const override {
        return "Flags UVM components and sequences without a comment of at least min_words (default "
               "10) words of prose right above the class. Copyright and license lines do not count. "
               "Advisory: off by default.";
    }
    std::string_view getCategory() const override { return "Advisory"; }
    core::Severity getDefaultSeverity() const override { return core::Severity::Info; }

    void run(slang::ast::Compilation& compilation, const uvm::UvmModel& model, core::DiagnosticEngine& diags) override {
        const auto* sm = compilation.getSourceManager();
        if (!sm) return;
        const auto minWords = swe::ruleOption("SV-RDB-010", "min_words", 10);
        for (const auto* cls : swe::userClasses(model)) {
            const auto role = swe::classRole(*cls);
            if (!swe::isComponentRole(role) && role != swe::Role::Sequence) continue;
            const auto* syntax = cls->getSyntax();
            if (!syntax || cls->genericClass) continue;
            const Token first = syntax->getFirstToken();
            if (isLibraryFile(*sm, first.location())) continue;
            if (proseWords(first) >= minWords) continue;
            diags.addViolation(
                "SV-RDB-010", "UndocumentedTestbenchClass",
                std::string(swe::roleName(role)) + " '" + std::string(cls->name) +
                    "' has no description. Two sentences above the class on what it models, drives "
                    "or checks save every reader a trip through the code.",
                core::Severity::Info, cls->location);
        }
    }

private:
    // Words in the comments directly above the class, skipping copyright and license lines.
    static int64_t proseWords(Token first) {
        int64_t words = 0;
        for (const auto& t : first.trivia()) {
            if (t.kind != TriviaKind::LineComment && t.kind != TriviaKind::BlockComment) continue;
            std::string_view text = t.getRawText();
            size_t start = 0;
            while (start < text.size()) {
                size_t end = text.find('\n', start);
                if (end == std::string_view::npos) end = text.size();
                const std::string line = lower(text.substr(start, end - start));
                start = end + 1;
                if (line.find("copyright") != std::string::npos || line.find("license") != std::string::npos ||
                    line.find("spdx") != std::string::npos) {
                    continue;
                }
                bool inWord = false;
                for (char c : line) {
                    const bool letter = std::isalpha(static_cast<unsigned char>(c));
                    if (letter && !inWord) ++words;
                    inWord = letter;
                }
            }
        }
        return words;
    }
};

// --- SV-SVA-006: properties too dense to read inline ----------------------------------

class ComplexPropertyScanner : public engine::SyntaxVisitor<ComplexPropertyScanner> {
public:
    ComplexPropertyScanner(const slang::SourceManager& sm, int64_t limit, core::DiagnosticEngine& diags) :
        sm(sm), limit(limit), diags(diags) {}

    void handle(const slang::syntax::ConcurrentAssertionStatementSyntax& stmt) {
        if (!isLibraryFile(sm, stmt.getFirstToken().location())) {
            static const std::set<std::string_view> temporal = {
                "##", "|->", "|=>", "[*", "[=", "[->", "throughout", "within", "intersect", "until",
                "s_until", "until_with", "s_until_with", "implies", "#-#", "#=#", "nexttime", "s_nexttime"};
            int64_t ops = 0;
            for (const Token& tok : tokensOf(*stmt.propertySpec)) {
                if (temporal.count(tok.rawText())) ++ops;
            }
            if (ops > limit) {
                diags.addViolation(
                    "SV-SVA-006", "ComplexInlineProperty",
                    "This assertion writes " + std::to_string(ops) +
                        " temporal operators inline. Name the pieces as sequence/property "
                        "declarations (s_addr_phase, s_data_phase) so the property reads like the "
                        "protocol's timing diagram.",
                    core::Severity::Info, stmt.getFirstToken().location());
            }
        }
        visitDefault(stmt);
    }

private:
    const slang::SourceManager& sm;
    int64_t limit;
    core::DiagnosticEngine& diags;
};

class ComplexInlineProperty : public Rule {
public:
    std::string_view getId() const override { return "SV-SVA-006"; }
    std::string_view getName() const override { return "ComplexInlineProperty"; }
    std::string_view getDescription() const override {
        return "Flags assert/assume/cover property statements whose inline property uses more than "
               "max_temporal_ops (default 3) temporal operators (##, |->, |=>, [*], throughout, "
               "within, until, ...). Advisory: off by default.";
    }
    std::string_view getCategory() const override { return "Advisory"; }
    core::Severity getDefaultSeverity() const override { return core::Severity::Info; }

    engine::RuleTask check(engine::RuleContext& ctx) override {
        ComplexPropertyScanner scanner(*ctx.compilation().getSourceManager(),
                                       swe::ruleOption("SV-SVA-006", "max_temporal_ops", 3), ctx.diags());
        co_await ctx.visitSyntax(scanner);
    }
};

} // namespace uvmcheck::rules::readability

namespace uvmcheck::rules {
std::unique_ptr<Rule> createDoubleNegativeFlag() {
    return std::make_unique<readability::DoubleNegativeFlag>();
}
std::unique_ptr<Rule> createNonDescriptiveName() {
    return std::make_unique<readability::NonDescriptiveName>();
}
std::unique_ptr<Rule> createUnclearAbbreviation() {
    return std::make_unique<readability::UnclearAbbreviation>();
}
std::unique_ptr<Rule> createRedundantComment() {
    return std::make_unique<readability::RedundantComment>();
}
std::unique_ptr<Rule> createUndocumentedTestbenchClass() {
    return std::make_unique<readability::UndocumentedTestbenchClass>();
}
std::unique_ptr<Rule> createComplexInlineProperty() {
    return std::make_unique<readability::ComplexInlineProperty>();
}
} // namespace uvmcheck::rules

UVMCHECK_REGISTER_RULE(uvmcheck::rules::createDoubleNegativeFlag)
UVMCHECK_REGISTER_RULE(uvmcheck::rules::createNonDescriptiveName)
UVMCHECK_REGISTER_RULE(uvmcheck::rules::createUnclearAbbreviation)
UVMCHECK_REGISTER_RULE(uvmcheck::rules::createRedundantComment)
UVMCHECK_REGISTER_RULE(uvmcheck::rules::createUndocumentedTestbenchClass)
UVMCHECK_REGISTER_RULE(uvmcheck::rules::createComplexInlineProperty)
