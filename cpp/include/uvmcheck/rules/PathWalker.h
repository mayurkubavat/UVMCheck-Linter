#pragma once

// A small path-sensitive walker over a routine body, shared by the flow rules
// (SV-RSC-001 lock release, SV-ERR-002 use after a failed null check).
//
// Each path carries a State. Loops are walked once (zero or one iteration), fork/join_none
// bodies are skipped because they run as separate threads, and `uvm_fatal / $fatal /
// $finish end a path. The number of distinct states is capped to keep it linear.
//
// Hooks must provide:
//   void leaf(const Statement&, State&)                      statements with no sub-statements
//   void branch(const Expression& cond, bool taken, State&)   refine state on a condition
//   void returned(const Statement& ret, const State&)         a path leaves through return

#include <algorithm>
#include <vector>

#include "slang/ast/Statement.h"
#include "slang/ast/statements/ConditionalStatements.h"
#include "slang/ast/statements/LoopStatements.h"
#include "slang/ast/statements/MiscStatements.h"
#include "uvmcheck/rules/SweSupport.h"

namespace uvmcheck::rules::swe {

template<typename State, typename Hooks>
class PathWalker {
public:
    using States = std::vector<State>;

    struct Out {
        States fall, brk, cont;
    };

    explicit PathWalker(Hooks& hooks) : hooks(hooks) {}

    /// Walks a routine body; returns the states that reach its end by falling through.
    States walkBody(const slang::ast::Statement& body, State init) {
        Out o = walk(body, States{std::move(init)});
        return o.fall;
    }

private:
    static constexpr size_t MaxStates = 32;
    Hooks& hooks;

    static void merge(States& into, const States& from) {
        for (const auto& s : from) {
            if (into.size() >= MaxStates) return;
            if (std::find(into.begin(), into.end(), s) == into.end()) into.push_back(s);
        }
    }

    static bool isReportGuard(const slang::ast::ConditionalStatement& c) { return swe::isReportGuard(c); }

    static bool stops(const slang::ast::Statement& s) {
        using namespace slang::ast;
        if (s.kind != StatementKind::ExpressionStatement) return false;
        const auto& e = stripConversions(s.as<ExpressionStatement>().expr);
        if (e.kind != ExpressionKind::Call) return false;
        const auto& call = e.as<CallExpression>();
        auto n = call.getSubroutineName();
        return n == "uvm_report_fatal" || n == "$fatal" || n == "$finish" || n == "$exit";
    }

    Out walk(const slang::ast::Statement& s, States in) {
        using namespace slang::ast;
        Out out;
        if (in.empty()) return out;
        switch (s.kind) {
            case StatementKind::List: {
                States curr = std::move(in);
                for (const auto* child : s.as<StatementList>().list) {
                    if (!child) continue;
                    Out o = walk(*child, std::move(curr));
                    merge(out.brk, o.brk);
                    merge(out.cont, o.cont);
                    curr = std::move(o.fall);
                    if (curr.empty()) break;
                }
                out.fall = std::move(curr);
                return out;
            }
            case StatementKind::Block: {
                const auto& b = s.as<BlockStatement>();
                if (b.blockKind == StatementBlockKind::JoinNone) {
                    out.fall = std::move(in);
                    return out;
                }
                return walk(b.body, std::move(in));
            }
            case StatementKind::Conditional: {
                const auto& c = s.as<ConditionalStatement>();
                if (isReportGuard(c)) return walk(c.ifTrue, std::move(in));
                States t = in, f = std::move(in);
                for (const auto& cond : c.conditions) {
                    for (auto& st : t) hooks.branch(*cond.expr, true, st);
                }
                if (c.conditions.size() == 1) {
                    for (auto& st : f) hooks.branch(*c.conditions[0].expr, false, st);
                }
                Out ot = walk(c.ifTrue, std::move(t));
                Out of;
                if (c.ifFalse) of = walk(*c.ifFalse, std::move(f));
                else of.fall = std::move(f);
                merge(out.fall, ot.fall); merge(out.fall, of.fall);
                merge(out.brk, ot.brk); merge(out.brk, of.brk);
                merge(out.cont, ot.cont); merge(out.cont, of.cont);
                return out;
            }
            case StatementKind::Case: {
                const auto& cs = s.as<CaseStatement>();
                for (const auto& item : cs.items) {
                    Out o = walk(*item.stmt, in);
                    merge(out.fall, o.fall); merge(out.brk, o.brk); merge(out.cont, o.cont);
                }
                if (cs.defaultCase) {
                    Out o = walk(*cs.defaultCase, in);
                    merge(out.fall, o.fall); merge(out.brk, o.brk); merge(out.cont, o.cont);
                } else {
                    merge(out.fall, in);
                }
                return out;
            }
            case StatementKind::ForLoop:
                return loop(s.as<ForLoopStatement>().body, std::move(in), true);
            case StatementKind::ForeachLoop:
                return loop(s.as<ForeachLoopStatement>().body, std::move(in), true);
            case StatementKind::WhileLoop:
                return loop(s.as<WhileLoopStatement>().body, std::move(in), true);
            case StatementKind::RepeatLoop:
                return loop(s.as<RepeatLoopStatement>().body, std::move(in), true);
            case StatementKind::DoWhileLoop:
                return loop(s.as<DoWhileLoopStatement>().body, std::move(in), false);
            case StatementKind::ForeverLoop: {
                Out o = walk(s.as<ForeverLoopStatement>().body, std::move(in));
                out.fall = std::move(o.brk);
                return out;
            }
            case StatementKind::Timed:
                return walk(s.as<TimedStatement>().stmt, std::move(in));
            case StatementKind::Wait:
                return walk(s.as<WaitStatement>().stmt, std::move(in));
            case StatementKind::Return:
                for (const auto& st : in) hooks.returned(s, st);
                return out;
            case StatementKind::Break:
                out.brk = std::move(in);
                return out;
            case StatementKind::Continue:
                out.cont = std::move(in);
                return out;
            default:
                if (stops(s)) return out;
                for (auto& st : in) hooks.leaf(s, st);
                // Dedupe after the hooks changed states.
                merge(out.fall, in);
                return out;
        }
    }

    Out loop(const slang::ast::Statement& body, States in, bool mayRunZeroTimes) {
        Out out;
        Out o = walk(body, in);
        if (mayRunZeroTimes) merge(out.fall, in);
        merge(out.fall, o.fall);
        merge(out.fall, o.cont);
        merge(out.fall, o.brk);
        return out;
    }
};

} // namespace uvmcheck::rules::swe
