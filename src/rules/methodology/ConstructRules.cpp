// Methodology guide chapter MG-6, construct usage: SV-CNS-001 to SV-CNS-008. Also SV-CPL-001
// (MG-2.4), which shares SV-CNS-004's string-dispatch check for routine arguments.

#include "MethodologySupport.h"
#include "uvmcheck/rules/Rule.h"
#include "uvmcheck/rules/RuleRegistry.h"

#include "slang/ast/ASTVisitor.h"
#include "slang/ast/expressions/AssignmentExpressions.h"
#include "slang/ast/expressions/CallExpression.h"
#include "slang/ast/expressions/OperatorExpressions.h"
#include "slang/ast/expressions/SelectExpressions.h"
#include "slang/ast/statements/ConditionalStatements.h"
#include "slang/ast/types/AllTypes.h"
#include "slang/syntax/AllSyntax.h"
#include "slang/syntax/SyntaxTree.h"
#include "slang/syntax/SyntaxVisitor.h"

#include <algorithm>
#include <cctype>
#include <set>
#include <unordered_map>
#include <unordered_set>

namespace uvmcheck::rules::methodology {

using namespace slang::ast;
using namespace slang::syntax;
using slang::parsing::Token;
using slang::parsing::TokenKind;
using slang::parsing::TriviaKind;

namespace {

/// Each macro defined in user code once, even when several trees include its header.
std::vector<const DefineDirectiveSyntax*> userMacros(Compilation& comp) {
    std::vector<const DefineDirectiveSyntax*> out;
    const auto* sm = comp.getSourceManager();
    if (!sm) return out;
    std::set<std::pair<std::string, size_t>> seen;
    for (const auto* tree : meth::userSyntaxTrees(comp)) {
        for (const auto* def : tree->getDefinedMacros()) {
            slang::SourceLocation loc = def->name.location();
            if (!loc.valid() || !sm->isFileLoc(loc) || meth::isLibraryBuffer(*sm, loc.buffer())) continue;
            if (def->name.valueText().starts_with("uvm_") || def->name.valueText().starts_with("UVM_")) continue;
            if (!seen.insert({std::string(sm->getRawFileName(loc.buffer())), loc.offset()}).second) continue;
            out.push_back(def);
        }
    }
    return out;
}

/// `ifndef NAME / `define NAME ... / `endif: a default that the build may override with
/// +define+NAME=..., which only a macro can offer.
bool isOverridableDefault(const slang::SourceManager& sm, const DefineDirectiveSyntax& def) {
    slang::SourceLocation loc = def.getFirstToken().location();
    std::string_view text = sm.getSourceText(loc.buffer());
    if (loc.offset() > text.size()) return false;
    std::string_view before = text.substr(0, loc.offset());
    // The previous non-blank line.
    size_t end = before.find_last_not_of(" \t\r\n");
    if (end == std::string_view::npos) return false;
    size_t start = before.find_last_of('\n', end);
    std::string_view line = before.substr(start == std::string_view::npos ? 0 : start + 1,
                                          end + 1 - (start == std::string_view::npos ? 0 : start + 1));
    size_t i = line.find_first_not_of(" \t");
    if (i == std::string_view::npos) return false;
    line.remove_prefix(i);
    if (!line.starts_with("`ifndef")) return false;
    line.remove_prefix(7);
    line.remove_prefix(std::min(line.find_first_not_of(" \t"), line.size()));
    std::string_view name = def.name.valueText();
    return line.starts_with(name) &&
           (line.size() == name.size() || !(std::isalnum(static_cast<unsigned char>(line[name.size()])) ||
                                            line[name.size()] == '_'));
}

void forEachToken(const SyntaxNode& node, const std::function<void(Token)>& fn) {
    for (size_t i = 0; i < node.getChildCount(); i++) {
        if (const auto* child = node.childNode(i)) {
            forEachToken(*child, fn);
        } else if (Token tok = node.childToken(i); tok) {
            fn(tok);
        }
    }
}

bool isNumberToken(TokenKind k) {
    // Not '0 / '1: a fill value has no width of its own, which a parameter cannot express.
    return k == TokenKind::IntegerLiteral || k == TokenKind::IntegerBase || k == TokenKind::RealLiteral ||
           k == TokenKind::TimeLiteral;
}

bool isFormalOf(const Symbol* sym, const SubroutineSymbol& sub) {
    if (!sym || sym->kind != SymbolKind::FormalArgument) return false;
    for (const auto* arg : sub.getArguments()) {
        if (arg == sym) return true;
    }
    return false;
}

} // namespace

// SV-CNS-001 ------------------------------------------------------------------------------

class DefineForConstant : public Rule {
public:
    std::string_view getId() const override { return "SV-CNS-001"; }
    std::string_view getName() const override { return "DefineForConstant"; }
    std::string_view getDescription() const override {
        return "Flags `define macros whose body is only a number or a string, used as constants. A "
               "package parameter or localparam has a type and a scope; a macro is global to the "
               "rest of the compilation and can be redefined silently (MG-6.1).";
    }
    std::string_view getCategory() const override { return "Maintainability"; }
    core::Severity getDefaultSeverity() const override { return core::Severity::Warning; }

    void run(Compilation& comp, const uvm::UvmModel&, core::DiagnosticEngine& diags) override {
        const auto* sm = comp.getSourceManager();
        for (const auto* def : userMacros(comp)) {
            if (def->formalArguments || def->body.empty() || isOverridableDefault(*sm, *def)) continue;
            int numbers = 0, strings = 0;
            bool constant = true;
            TokenKind prev = TokenKind::Unknown;
            for (Token t : def->body) {
                TokenKind k = t.kind;
                if (isNumberToken(k)) numbers++;
                else if (k == TokenKind::StringLiteral) strings++;
                else if (k == TokenKind::Identifier && prev == TokenKind::IntegerBase) numbers++; // 'hFF
                else if (k != TokenKind::OpenParenthesis && k != TokenKind::CloseParenthesis &&
                         k != TokenKind::Minus && k != TokenKind::Plus && k != TokenKind::Star &&
                         k != TokenKind::Slash && k != TokenKind::LeftShift) {
                    constant = false;
                }
                prev = k;
            }
            if (!constant || (numbers == 0) == (strings == 0) || strings > 1) continue;
            std::string body;
            for (Token t : def->body) body += std::string(t.rawText());
            diags.addViolation(
                "SV-CNS-001", "DefineForConstant",
                "`define " + std::string(def->name.valueText()) + " holds the constant '" + body +
                    "'. As a macro it has no type, no scope and can be redefined by any later file. "
                    "Declare it as a parameter or localparam in a package instead.",
                core::Severity::Warning, def->name.location());
        }
    }
};

// SV-CNS-002 ------------------------------------------------------------------------------

namespace {

bool isDeclarationKeyword(TokenKind k) {
    switch (k) {
        case TokenKind::ClassKeyword:
        case TokenKind::FunctionKeyword:
        case TokenKind::TaskKeyword:
        case TokenKind::TypedefKeyword:
        case TokenKind::CoverGroupKeyword:
        case TokenKind::ConstraintKeyword:
        case TokenKind::ModuleKeyword:
        case TokenKind::InterfaceKeyword:
        case TokenKind::PropertyKeyword:
        case TokenKind::SequenceKeyword:
        case TokenKind::RandKeyword:
        case TokenKind::RandCKeyword:
        case TokenKind::LocalKeyword:
        case TokenKind::ProtectedKeyword:
        case TokenKind::StaticKeyword:
        case TokenKind::VirtualKeyword:
        case TokenKind::AssertKeyword:
        case TokenKind::CoverKeyword:
        case TokenKind::ImportKeyword:
        // Module items: a routine cannot contain these.
        case TokenKind::AlwaysKeyword:
        case TokenKind::AlwaysCombKeyword:
        case TokenKind::AlwaysFFKeyword:
        case TokenKind::AlwaysLatchKeyword:
        case TokenKind::InitialKeyword:
        case TokenKind::FinalKeyword:
        case TokenKind::AssignKeyword:
        case TokenKind::LocalParamKeyword:
        case TokenKind::ParameterKeyword:
        case TokenKind::GenerateKeyword:
            return true;
        default:
            return false;
    }
}

bool affectsCaller(TokenKind k) {
    return k == TokenKind::ReturnKeyword || k == TokenKind::BreakKeyword ||
           k == TokenKind::ContinueKeyword || k == TokenKind::DisableKeyword;
}

bool isMacroOnlyToken(TokenKind k) {
    // Token pasting, stringification, nested macros and `__FILE__/`__LINE__ are things only
    // a macro can do.
    return k == TokenKind::Directive || k == TokenKind::MacroUsage || k == TokenKind::MacroPaste ||
           k == TokenKind::MacroQuote || k == TokenKind::MacroEscapedQuote ||
           k == TokenKind::MacroTripleQuote;
}

} // namespace

class MacroAsRoutine : public Rule {
public:
    std::string_view getId() const override { return "SV-CNS-002"; }
    std::string_view getName() const override { return "MacroAsRoutine"; }
    std::string_view getDescription() const override {
        return "Flags multi-statement code macros that use nothing only a macro can do: no token "
               "pasting, stringification, nested macros, `__FILE__, type arguments, declarations "
               "or return/break. A function or task would be type-checked and debuggable (MG-6.1). "
               "Advisory: off by default.";
    }
    std::string_view getCategory() const override { return "Advisory"; }
    core::Severity getDefaultSeverity() const override { return core::Severity::Info; }

    void run(Compilation& comp, const uvm::UvmModel&, core::DiagnosticEngine& diags) override {
        const auto* sm = comp.getSourceManager();
        for (const auto* def : userMacros(comp)) {
            if (isOverridableDefault(*sm, *def)) continue;
            std::unordered_set<std::string_view> formals;
            if (def->formalArguments) {
                for (const auto* arg : def->formalArguments->args) formals.insert(arg->name.valueText());
            }
            int semis = 0, depth = 0;
            bool hasBegin = false, macroOnly = false;
            auto body = def->body;
            for (size_t i = 0; i < body.size() && !macroOnly; i++) {
                TokenKind k = body[i].kind;
                if (isMacroOnlyToken(k) || isDeclarationKeyword(k) || affectsCaller(k)) macroOnly = true;
                if (k == TokenKind::Identifier && formals.count(body[i].valueText()) && i + 1 < body.size() &&
                    (body[i + 1].kind == TokenKind::DoubleColon || body[i + 1].kind == TokenKind::Hash)) {
                    macroOnly = true; // a type argument
                }
                if (k == TokenKind::OpenParenthesis || k == TokenKind::OpenBrace) depth++;
                if (k == TokenKind::CloseParenthesis || k == TokenKind::CloseBrace) depth--;
                if (k == TokenKind::Semicolon && depth == 0) semis++;
                if (k == TokenKind::BeginKeyword || k == TokenKind::ForkKeyword) hasBegin = true;
            }
            if (macroOnly || !(semis >= 2 || (hasBegin && semis >= 1))) continue;
            std::string kind = formals.empty() ? "function or task" : "function or task with arguments";
            diags.addViolation(
                "SV-CNS-002", "MacroAsRoutine",
                "`define " + std::string(def->name.valueText()) + " is a block of statements that "
                    "uses nothing only a macro can do. Write it as a " + kind +
                    ": arguments are type-checked, it can be stepped through in a debugger, and "
                    "errors point at its own lines instead of every expansion.",
                core::Severity::Info, def->name.location());
        }
    }
};

// SV-CNS-003 ------------------------------------------------------------------------------

namespace {

bool isShimMacro(std::string_view n) {
    static const std::vector<std::string_view> prefixes = {
        "UVM_", "VCS", "QUESTA", "MODEL_TECH", "MTI", "XCELIUM", "INCA", "NCSIM", "CADENCE",
        "RIVIERA", "ALDEC", "VERILATOR", "SYNTHESIS", "SYNOPSYS", "IUS", "DSIM", "IVERILOG"};
    for (auto p : prefixes) {
        if (n.starts_with(p)) return true;
    }
    return false;
}

void macroNames(const SyntaxNode& expr, std::vector<std::string_view>& out) {
    forEachToken(expr, [&](Token t) {
        if (t.kind == TokenKind::Identifier) out.push_back(t.valueText());
    });
}

class IfdefVisitor : public SyntaxVisitor<IfdefVisitor> {
public:
    IfdefVisitor(const slang::SourceManager& sm, core::DiagnosticEngine& diags) : sm(sm), diags(diags) {}

    void handle(const ClassDeclarationSyntax& cls) {
        Token first = cls.getFirstToken();
        if (sm.isFileLoc(first.location()) && !meth::isLibraryBuffer(sm, first.location().buffer())) {
            std::string name(cls.name.valueText());
            bool skipFirst = true;
            forEachToken(cls, [&](Token t) {
                if (skipFirst) { skipFirst = false; return; }
                for (const auto& trivia : t.trivia()) {
                    if (trivia.kind != TriviaKind::Directive) continue;
                    const auto* d = trivia.syntax();
                    if (!d || (d->kind != SyntaxKind::IfDefDirective && d->kind != SyntaxKind::IfNDefDirective &&
                               d->kind != SyntaxKind::ElsIfDirective)) {
                        continue;
                    }
                    if (!seen.insert(d).second) continue;
                    std::vector<std::string_view> names;
                    macroNames(*d->as<ConditionalBranchDirectiveSyntax>().expr, names);
                    bool shim = names.empty();
                    for (auto n : names) shim = shim || isShimMacro(n);
                    if (shim) continue;
                    std::string what = names.size() == 1 ? "`" + std::string(names[0]) : "macros";
                    diags.addViolation(
                        "SV-CNS-003", "IfdefInReusableClass",
                        "Class '" + name + "' changes its members or code with " + what +
                            ". Each define gives a different class that has to be compiled and "
                            "tested on its own, and a reusable component cannot be configured "
                            "per instance this way. Use a config object field, a parameter or a "
                            "factory override.",
                        core::Severity::Warning, d->getFirstToken().location());
                }
            });
        }
        visitDefault(cls);
    }

private:
    const slang::SourceManager& sm;
    core::DiagnosticEngine& diags;
    std::unordered_set<const SyntaxNode*> seen;
};

} // namespace

class IfdefInReusableClass : public Rule {
public:
    std::string_view getId() const override { return "SV-CNS-003"; }
    std::string_view getName() const override { return "IfdefInReusableClass"; }
    std::string_view getDescription() const override {
        return "Flags `ifdef, `ifndef and `elsif inside class bodies, apart from simulator and UVM "
               "version shims. Compile-time switches multiply the variants of a class and cannot be "
               "chosen per instance; configuration can (MG-6.1).";
    }
    std::string_view getCategory() const override { return "Maintainability"; }
    core::Severity getDefaultSeverity() const override { return core::Severity::Warning; }

    void run(Compilation& comp, const uvm::UvmModel&, core::DiagnosticEngine& diags) override {
        const auto* sm = comp.getSourceManager();
        if (!sm) return;
        IfdefVisitor v(*sm, diags);
        for (const auto* tree : meth::userSyntaxTrees(comp)) tree->root().visit(v);
    }
};

// SV-CNS-004 ------------------------------------------------------------------------------

namespace {

/// `x == "lit"` pairs (also through ||) where x is a string-typed value.
void stringComparisons(const Expression& e, std::vector<std::pair<const Symbol*, std::string>>& out) {
    const Expression& x = swe::stripConversions(e);
    if (x.kind != ExpressionKind::BinaryOp) return;
    const auto& b = x.as<BinaryExpression>();
    if (b.op == BinaryOperator::LogicalOr) {
        stringComparisons(b.left(), out);
        stringComparisons(b.right(), out);
        return;
    }
    if (b.op != BinaryOperator::Equality && b.op != BinaryOperator::CaseEquality) return;
    const Expression* lit = &b.right();
    const Expression* val = &b.left();
    if (!meth::isStringLiteral(*lit)) std::swap(lit, val);
    if (!meth::isStringLiteral(*lit) || meth::isStringLiteral(*val)) return;
    const Expression& v = swe::stripConversions(*val);
    if (!v.type || !v.type->isString()) return;
    if (const Symbol* sym = swe::referencedSymbol(v)) out.push_back({sym, meth::literalValue(*lit)});
}

class StringDispatchVisitor : public ASTVisitor<StringDispatchVisitor, VisitFlags::AllGood> {
public:
    StringDispatchVisitor(const SubroutineSymbol& sub, bool argsOnly, size_t threshold,
                          std::function<void(const Symbol&, const std::set<std::string>&, slang::SourceLocation)> report) :
        sub(sub), argsOnly(argsOnly), threshold(threshold), report(std::move(report)) {}

    void handle(const CaseStatement& c) {
        const Expression& cond = swe::stripConversions(c.expr);
        const Symbol* sym = cond.type && cond.type->isString() ? swe::referencedSymbol(cond) : nullptr;
        if (sym && wanted(sym)) {
            std::set<std::string> lits;
            for (const auto& item : c.items) {
                for (const auto* e : item.expressions) {
                    if (meth::isStringLiteral(*e)) lits.insert(meth::literalValue(*e));
                }
            }
            lits.erase("");
            if (lits.size() >= threshold) report(*sym, lits, swe::locOf(c));
        }
        visitDefault(c);
    }

    void handle(const ConditionalStatement& c) {
        if (!inChain.count(&c)) {
            std::vector<const Symbol*> order;
            std::unordered_map<const Symbol*, std::set<std::string>> lits;
            for (const ConditionalStatement* s = &c; s;) {
                inChain.insert(s);
                for (const auto& cond : s->conditions) {
                    std::vector<std::pair<const Symbol*, std::string>> pairs;
                    stringComparisons(*cond.expr, pairs);
                    for (auto& [sym, lit] : pairs) {
                        if (!wanted(sym) || lit.empty()) continue;
                        if (!lits.count(sym)) order.push_back(sym);
                        lits[sym].insert(lit);
                    }
                }
                s = s->ifFalse && s->ifFalse->kind == StatementKind::Conditional
                        ? &s->ifFalse->as<ConditionalStatement>()
                        : nullptr;
            }
            for (const auto* sym : order) {
                if (lits[sym].size() >= threshold) {
                    report(*sym, lits[sym], swe::locOf(c));
                    break;
                }
            }
        }
        visitDefault(c);
    }

    void handle(const ConditionalExpression& c) {
        if (!inChainExpr.count(&c)) {
            std::vector<const Symbol*> order;
            std::unordered_map<const Symbol*, std::set<std::string>> lits;
            for (const ConditionalExpression* e = &c; e;) {
                inChainExpr.insert(e);
                for (const auto& cond : e->conditions) {
                    std::vector<std::pair<const Symbol*, std::string>> pairs;
                    stringComparisons(*cond.expr, pairs);
                    for (auto& [sym, lit] : pairs) {
                        if (!wanted(sym) || lit.empty()) continue;
                        if (!lits.count(sym)) order.push_back(sym);
                        lits[sym].insert(lit);
                    }
                }
                const Expression& f = swe::stripConversions(e->right());
                e = f.kind == ExpressionKind::ConditionalOp ? &f.as<ConditionalExpression>() : nullptr;
            }
            for (const auto* sym : order) {
                if (lits[sym].size() >= threshold) {
                    report(*sym, lits[sym], swe::locOf(c));
                    break;
                }
            }
        }
        visitDefault(c);
    }

private:
    const SubroutineSymbol& sub;
    bool argsOnly;
    size_t threshold;
    std::function<void(const Symbol&, const std::set<std::string>&, slang::SourceLocation)> report;
    std::unordered_set<const ConditionalStatement*> inChain;
    std::unordered_set<const ConditionalExpression*> inChainExpr;

    // Arguments of the routine are SV-CPL-001's; everything else is SV-CNS-004's.
    bool wanted(const Symbol* sym) const { return isFormalOf(sym, sub) == argsOnly; }
};

/// UVM names register access policies with strings (uvm_reg_field::get_access()), so code
/// that dispatches on them has no enum to use.
bool allUvmAccessPolicies(const std::set<std::string>& lits) {
    static const std::set<std::string_view> policies = {
        "RO",   "RW",   "RC",    "RS",    "WRC",   "WRS",   "WC",    "WS",   "WSRC", "WCRS",
        "W1C",  "W1S",  "W1T",   "W0C",   "W0S",   "W0T",   "W1SRC", "W1CRS", "W0SRC", "W0CRS",
        "WO",   "WOC",  "WOS",   "W1",    "WO1",   "NOACCESS"};
    return std::all_of(lits.begin(), lits.end(), [](const std::string& l) { return policies.count(l) > 0; });
}

/// Variables filled from a UVM library call (s = f.get_access()): the string comes from an API
/// the user does not control.
class LibraryValueScanner : public ASTVisitor<LibraryValueScanner, VisitFlags::AllGood> {
public:
    std::unordered_set<const Symbol*> fromLibrary;

    void handle(const AssignmentExpression& a) {
        const Expression& rhs = swe::stripConversions(a.right());
        if (rhs.kind == ExpressionKind::Call) {
            const auto& call = rhs.as<CallExpression>();
            const auto* sub = std::get_if<const SubroutineSymbol*>(&call.subroutine);
            if (!call.isSystemCall() && sub && *sub && swe::isLibrarySymbol(**sub)) {
                if (const Symbol* s = swe::referencedSymbol(swe::stripConversions(a.left()))) fromLibrary.insert(s);
            }
        }
        visitDefault(a);
    }
};

std::string literalList(const std::set<std::string>& lits) {
    std::string out;
    size_t n = 0;
    for (const auto& l : lits) {
        if (n++ == 4) { out += ", ..."; break; }
        out += (out.empty() ? "\"" : ", \"") + l + "\"";
    }
    return out;
}

} // namespace

class StringAsEnum : public Rule {
public:
    std::string_view getId() const override { return "SV-CNS-004"; }
    std::string_view getName() const override { return "StringAsEnum"; }
    std::string_view getDescription() const override {
        return "Flags a string variable compared against three or more literals in one case "
               "statement or if/else-if chain. The values form an enumeration the compiler cannot "
               "check; a typo in any of them fails silently (Fowler 'primitive obsession', MG-6.2). "
               "String arguments are SV-CPL-001's.";
    }
    std::string_view getCategory() const override { return "Maintainability"; }
    core::Severity getDefaultSeverity() const override { return core::Severity::Warning; }

    void run(Compilation& comp, const uvm::UvmModel& model, core::DiagnosticEngine& diags) override {
        size_t threshold = static_cast<size_t>(swe::ruleOption("SV-CNS-004", "min_values", 3));
        for (const auto* sub : swe::allUserRoutines(comp, model)) {
            LibraryValueScanner scan;
            sub->getBody().visit(scan);
            StringDispatchVisitor v(*sub, false, threshold,
                                    [&](const Symbol& sym, const std::set<std::string>& lits, slang::SourceLocation loc) {
                if (scan.fromLibrary.count(&sym) || allUvmAccessPolicies(lits)) return;
                diags.addViolation(
                    "SV-CNS-004", "StringAsEnum",
                    "String '" + std::string(sym.name) + "' is compared against " +
                        std::to_string(lits.size()) + " literals (" + literalList(lits) +
                        ") to choose what to do. The compiler cannot catch a misspelt or missing "
                        "value. Declare an enum and convert the string once where it enters, for "
                        "example with uvm_enum_wrapper#(T)::from_name().",
                    core::Severity::Warning, loc);
            });
            sub->getBody().visit(v);
        }
    }
};

// SV-CNS-005 ------------------------------------------------------------------------------

namespace {

struct SliceUse {
    const Symbol* sym = nullptr;
    std::set<std::string> ranges;
    slang::SourceLocation first;
};

class SliceVisitor : public ASTVisitor<SliceVisitor, VisitFlags::AllGood> {
public:
    SliceVisitor(std::vector<SliceUse>& uses, std::unordered_map<const Symbol*, size_t>& index) :
        uses(uses), index(index) {}

    void handle(const RangeSelectExpression& e) {
        if (e.getSelectionKind() == RangeSelectionKind::Simple && e.value().type) {
            const Type& t = e.value().type->getCanonicalType();
            const Expression& v = swe::stripConversions(e.value());
            const Symbol* sym = (v.kind == ExpressionKind::NamedValue || v.kind == ExpressionKind::MemberAccess)
                                    ? swe::referencedSymbol(v)
                                    : nullptr;
            if (sym && t.isIntegral() && !t.isStruct() && !t.isPackedUnion() &&
                !swe::isLibrarySymbol(*sym)) {
                auto [it, added] = index.try_emplace(sym, uses.size());
                if (added) uses.push_back({sym, {}, swe::locOf(e)});
                uses[it->second].ranges.insert(swe::exprText(e.left()) + ":" + swe::exprText(e.right()));
            }
        }
        visitDefault(e);
    }

private:
    std::vector<SliceUse>& uses;
    std::unordered_map<const Symbol*, size_t>& index;
};

} // namespace

class ManualBitFieldSlicing : public Rule {
public:
    std::string_view getId() const override { return "SV-CNS-005"; }
    std::string_view getName() const override { return "ManualBitFieldSlicing"; }
    std::string_view getDescription() const override {
        return "Flags a vector that is cut into three or more different fixed part-selects "
               "(hdr[7:0], hdr[15:8], hdr[31:16]). The field layout lives in scattered numbers; a "
               "packed struct names it once (MG-6.2). Advisory: off by default.";
    }
    std::string_view getCategory() const override { return "Advisory"; }
    core::Severity getDefaultSeverity() const override { return core::Severity::Info; }

    void run(Compilation& comp, const uvm::UvmModel& model, core::DiagnosticEngine& diags) override {
        size_t threshold = static_cast<size_t>(swe::ruleOption("SV-CNS-005", "min_fields", 3));
        std::vector<SliceUse> uses;
        std::unordered_map<const Symbol*, size_t> index;
        SliceVisitor v(uses, index);
        for (const auto* sub : swe::allUserRoutines(comp, model)) sub->getBody().visit(v);
        for (const auto& u : uses) {
            if (u.ranges.size() < threshold) continue;
            std::string list;
            for (const auto& r : u.ranges) list += (list.empty() ? "[" : ", [") + r + "]";
            diags.addViolation(
                "SV-CNS-005", "ManualBitFieldSlicing",
                "'" + std::string(u.sym->name) + "' is sliced into " + std::to_string(u.ranges.size()) +
                    " fixed fields (" + list + "). The layout is spread over these numbers and "
                    "breaks quietly when one changes. Declare a packed struct with named fields and "
                    "use it (or cast to it) instead.",
                core::Severity::Info, u.first);
        }
    }
};

// SV-CNS-006 and SV-CNS-007 ---------------------------------------------------------------

namespace {

enum class Container { None, Class, Package, Interface, Module };

Container containerOf(const SyntaxNode& node) {
    for (const SyntaxNode* p = node.parent; p; p = p->parent) {
        switch (p->kind) {
            case SyntaxKind::ClassDeclaration: return Container::Class;
            case SyntaxKind::PackageDeclaration: return Container::Package;
            case SyntaxKind::InterfaceDeclaration: return Container::Interface;
            case SyntaxKind::ModuleDeclaration:
            case SyntaxKind::ProgramDeclaration: return Container::Module;
            default: break;
        }
    }
    return Container::None;
}

template<typename F>
class TypeSyntaxVisitor : public SyntaxVisitor<TypeSyntaxVisitor<F>> {
public:
    TypeSyntaxVisitor(const slang::SourceManager& sm, F fn) : sm(sm), fn(std::move(fn)) {}

    void handle(const EnumTypeSyntax& e) {
        if (user(e)) fn(e);
        this->visitDefault(e);
    }
    void handle(const IntegerTypeSyntax& t) {
        if (user(t)) fn(t);
        this->visitDefault(t);
    }

private:
    const slang::SourceManager& sm;
    F fn;
    bool user(const SyntaxNode& n) const {
        slang::SourceLocation loc = n.getFirstToken().location();
        return sm.isFileLoc(loc) && !meth::isLibraryBuffer(sm, loc.buffer());
    }
};

template<typename F>
void visitTypeSyntax(Compilation& comp, F fn) {
    const auto* sm = comp.getSourceManager();
    if (!sm) return;
    TypeSyntaxVisitor<F> v(*sm, std::move(fn));
    for (const auto* tree : meth::userSyntaxTrees(comp)) tree->root().visit(v);
}

} // namespace

class ImplicitEnumBaseType : public Rule {
public:
    std::string_view getId() const override { return "SV-CNS-006"; }
    std::string_view getName() const override { return "ImplicitEnumBaseType"; }
    std::string_view getDescription() const override {
        return "Flags enums in packages, classes and interfaces declared without a base type. They "
               "silently become 32-bit int, which wastes bits in packed items and no longer matches "
               "the width of the bus field they model (MG-6.3). "
               "Advisory: off by default.";
    }
    std::string_view getCategory() const override { return "Advisory"; }
    core::Severity getDefaultSeverity() const override { return core::Severity::Info; }

    void run(Compilation& comp, const uvm::UvmModel&, core::DiagnosticEngine& diags) override {
        visitTypeSyntax(comp, [&](const SyntaxNode& node) {
            if (node.kind != SyntaxKind::EnumType || node.as<EnumTypeSyntax>().baseType) return;
            Container c = containerOf(node);
            if (c == Container::Module) return;
            const auto& e = node.as<EnumTypeSyntax>();
            std::string first = e.members.empty() ? std::string() : std::string(e.members[0]->name.valueText());
            diags.addViolation(
                "SV-CNS-006", "ImplicitEnumBaseType",
                "Enum" + (first.empty() ? std::string() : " {" + first + ", ...}") +
                    " has no base type, so it is a 32-bit int. State the width it models, for "
                    "example enum bit [1:0] {...}.",
                core::Severity::Info, e.keyword.location());
        });
    }
};

class LegacyIntegerType : public Rule {
public:
    std::string_view getId() const override { return "SV-CNS-007"; }
    std::string_view getName() const override { return "LegacyIntegerType"; }
    std::string_view getDescription() const override {
        return "Flags the Verilog-1995 type 'integer' in class code. It is a 4-state 32-bit signed "
               "value; int (2-state) or an explicitly sized logic vector says what is meant (MG-6.3). "
               "Advisory: off by default.";
    }
    std::string_view getCategory() const override { return "Advisory"; }
    core::Severity getDefaultSeverity() const override { return core::Severity::Info; }

    void run(Compilation& comp, const uvm::UvmModel&, core::DiagnosticEngine& diags) override {
        visitTypeSyntax(comp, [&](const SyntaxNode& node) {
            if (node.kind != SyntaxKind::IntegerType || containerOf(node) != Container::Class) return;
            diags.addViolation(
                "SV-CNS-007", "LegacyIntegerType",
                "'integer' in class code is a 4-state, signed 32-bit legacy type. Use int for "
                "counters and indices, or logic [N-1:0] when X and Z must be kept.",
                core::Severity::Info, node.getFirstToken().location());
        });
    }
};

// SV-CNS-008 ------------------------------------------------------------------------------

class FunctionOutputArguments : public Rule {
public:
    std::string_view getId() const override { return "SV-CNS-008"; }
    std::string_view getName() const override { return "FunctionOutputArguments"; }
    std::string_view getDescription() const override {
        return "Flags void class functions that hand their results back through output arguments. "
               "A return value, or a struct for several values, keeps the call an expression and "
               "makes the data flow visible (command-query separation, MG-6.3). Overrides are left "
               "alone, since their signature is fixed. Advisory: off by default.";
    }
    std::string_view getCategory() const override { return "Advisory"; }
    core::Severity getDefaultSeverity() const override { return core::Severity::Info; }

    void run(Compilation&, const uvm::UvmModel& model, core::DiagnosticEngine& diags) override {
        for (const auto* cls : swe::userClasses(model)) {
            for (const auto* m : swe::classMethods(*cls)) {
                if (m->subroutineKind != SubroutineKind::Function || !m->getReturnType().isVoid()) continue;
                if (meth::overridesBaseMethod(*m)) continue;
                std::vector<std::string_view> outs;
                for (const auto* arg : m->getArguments()) {
                    if (arg->direction == ArgumentDirection::Out) outs.push_back(arg->name);
                }
                if (outs.empty()) continue;
                std::string list;
                for (auto n : outs) list += (list.empty() ? "'" : ", '") + std::string(n) + "'";
                diags.addViolation(
                    "SV-CNS-008", "FunctionOutputArguments",
                    "void function '" + std::string(cls->name) + "::" + std::string(m->name) +
                        "' returns its result through output argument" + (outs.size() > 1 ? "s " : " ") +
                        list + ". " +
                        (outs.size() > 1 ? "Return a struct that holds them instead."
                                         : "Return the value instead."),
                    core::Severity::Info, m->location);
            }
        }
    }
};

// SV-CPL-001 ------------------------------------------------------------------------------

class StringModeParameter : public Rule {
public:
    std::string_view getId() const override { return "SV-CPL-001"; }
    std::string_view getName() const override { return "StringModeParameter"; }
    std::string_view getDescription() const override {
        return "Flags routines whose string argument is compared against two or more literals to "
               "choose what to do, as in do_op(\"read\"). Caller and callee must agree on spellings "
               "the compiler never checks (control coupling, connascence of meaning, MG-2.4). Use an "
               "enum argument or separate routines. Overrides are left alone.";
    }
    std::string_view getCategory() const override { return "Maintainability"; }
    core::Severity getDefaultSeverity() const override { return core::Severity::Warning; }

    void run(Compilation& comp, const uvm::UvmModel& model, core::DiagnosticEngine& diags) override {
        size_t threshold = static_cast<size_t>(swe::ruleOption("SV-CPL-001", "min_values", 2));
        for (const auto* sub : swe::allUserRoutines(comp, model)) {
            if (meth::overridesBaseMethod(*sub)) continue;
            std::unordered_set<const Symbol*> reported;
            StringDispatchVisitor v(*sub, true, threshold,
                                    [&](const Symbol& sym, const std::set<std::string>& lits, slang::SourceLocation) {
                if (allUvmAccessPolicies(lits) || !reported.insert(&sym).second) return;
                diags.addViolation(
                    "SV-CPL-001", "StringModeParameter",
                    "'" + std::string(sub->name) + "' chooses its behaviour from the string argument '" +
                        std::string(sym.name) + "' (" + literalList(lits) + "). Every caller has to "
                        "spell these values exactly, and a typo is not caught. Take an enum argument, "
                        "or split the routine into one routine per mode.",
                    core::Severity::Warning, sym.location);
            });
            sub->getBody().visit(v);
        }
    }
};

} // namespace uvmcheck::rules::methodology

namespace uvmcheck::rules {
std::unique_ptr<Rule> createStringModeParameter() { return std::make_unique<methodology::StringModeParameter>(); }
std::unique_ptr<Rule> createDefineForConstant() { return std::make_unique<methodology::DefineForConstant>(); }
std::unique_ptr<Rule> createMacroAsRoutine() { return std::make_unique<methodology::MacroAsRoutine>(); }
std::unique_ptr<Rule> createIfdefInReusableClass() { return std::make_unique<methodology::IfdefInReusableClass>(); }
std::unique_ptr<Rule> createStringAsEnum() { return std::make_unique<methodology::StringAsEnum>(); }
std::unique_ptr<Rule> createManualBitFieldSlicing() { return std::make_unique<methodology::ManualBitFieldSlicing>(); }
std::unique_ptr<Rule> createImplicitEnumBaseType() { return std::make_unique<methodology::ImplicitEnumBaseType>(); }
std::unique_ptr<Rule> createLegacyIntegerType() { return std::make_unique<methodology::LegacyIntegerType>(); }
std::unique_ptr<Rule> createFunctionOutputArguments() { return std::make_unique<methodology::FunctionOutputArguments>(); }
}
