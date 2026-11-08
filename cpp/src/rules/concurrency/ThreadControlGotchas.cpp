#include "uvmcheck/rules/Rule.h"
#include "uvmcheck/rules/RuleRegistry.h"
#include "uvmcheck/rules/SweSupport.h"
#include "slang/ast/ASTVisitor.h"
#include "slang/ast/TimingControl.h"
#include "slang/ast/expressions/MiscExpressions.h"
#include "slang/ast/statements/MiscStatements.h"
#include "slang/ast/symbols/BlockSymbols.h"
#include <algorithm>
#include <cctype>
#include <optional>
#include <string>

namespace uvmcheck::rules::concurrency {

using namespace slang::ast;

// --- SV-CON-014: disable <named block> inside a class method ------------------------

class DisableNamedBlockWalker : public ASTVisitor<DisableNamedBlockWalker, VisitFlags::AllGood> {
public:
    DisableNamedBlockWalker(const ClassType& cls, core::DiagnosticEngine& diags) : cls(cls), diags(diags) {}

    void handle(const DisableStatement& stmt) {
        const Symbol* target = stmt.target.kind == ExpressionKind::ArbitrarySymbol
                                   ? stmt.target.as<ArbitrarySymbolExpression>().symbol.get()
                                   : swe::referencedSymbol(stmt.target);
        if (target && (target->kind == SymbolKind::StatementBlock || target->kind == SymbolKind::Subroutine)) {
            const bool isBlock = target->kind == SymbolKind::StatementBlock;
            diags.addViolation(
                "SV-CON-014", "DisableNamedBlockInClass",
                std::string("'disable ") + std::string(target->name) + "' in class '" + std::string(cls.name) +
                    "' stops that " + (isBlock ? "block" : "task") +
                    " in every object of the class, not just this one. "
                    "Keep a process handle (process::self()) and call kill() on it, "
                    "or use break/continue/return to leave a loop or task.",
                core::Severity::Warning, stmt.sourceRange.start());
        }
        visitDefault(stmt);
    }

private:
    const ClassType& cls;
    core::DiagnosticEngine& diags;
};

class DisableNamedBlockInClass : public Rule {
public:
    std::string_view getId() const override { return "SV-CON-014"; }
    std::string_view getName() const override { return "DisableNamedBlockInClass"; }
    std::string_view getDescription() const override {
        return "Flags 'disable <block>' and 'disable <task>' inside class methods. A named-block or "
               "task disable acts on every running copy, so stopping one agent's thread also stops "
               "the same thread in all other instances. 'disable fork' is not flagged.";
    }
    std::string_view getCategory() const override { return "Concurrency"; }
    core::Severity getDefaultSeverity() const override { return core::Severity::Warning; }

    void run(slang::ast::Compilation&, const uvm::UvmModel& model, core::DiagnosticEngine& diags) override {
        for (const auto* cls : swe::userClasses(model)) {
            DisableNamedBlockWalker walker(*cls, diags);
            for (const auto* method : swe::classMethods(*cls)) {
                if (method->getParentScope() == cls) method->getBody().visit(walker);
            }
        }
    }
};

// --- SV-CON-015: unguarded wait on an interface signal in a driver or sequence -------

std::string lower(std::string_view s) {
    std::string out(s);
    std::transform(out.begin(), out.end(), out.begin(), [](unsigned char c) { return std::tolower(c); });
    return out;
}

bool isExemptSignal(std::string_view name) {
    const std::string n = lower(name);
    for (const char* key : {"clk", "clock", "rst", "reset"}) {
        if (n.find(key) != std::string::npos) return true;
    }
    return false;
}

// The interface signal an event or wait condition reads ("" for clocking-block events
// and clock or reset signals, nullopt when it reads no virtual interface).
class VifSignalFinder : public ASTVisitor<VifSignalFinder, VisitFlags::AllGood> {
public:
    std::optional<std::string> signal;

    void handle(const MemberAccessExpression& m) {
        if (signal) return;
        const Expression* root = &m.value();
        while (root->kind == ExpressionKind::MemberAccess) root = &root->as<MemberAccessExpression>().value();
        if (root->type && root->type->isVirtualInterface()) {
            const bool exempt = m.member.kind == SymbolKind::ClockingBlock || isExemptSignal(m.member.name);
            signal = exempt ? std::string() : std::string(m.member.name);
            return;
        }
        visitDefault(m);
    }
};

std::optional<std::string> vifSignal(const Expression& expr) {
    VifSignalFinder finder;
    expr.visit(finder);
    return finder.signal;
}

class UnguardedWaitWalker : public ASTVisitor<UnguardedWaitWalker, VisitFlags::AllGood> {
public:
    UnguardedWaitWalker(const ClassType& cls, core::DiagnosticEngine& diags) : cls(cls), diags(diags) {}

    // A wait inside fork ... join_any races another branch (a delay, a watchdog event),
    // which is the usual timeout idiom.
    void handle(const BlockStatement& block) {
        const bool racing = block.blockKind == StatementBlockKind::JoinAny;
        guarded += racing;
        visitDefault(block);
        guarded -= racing;
    }

    void handle(const WaitStatement& stmt) {
        check(stmt.cond, stmt.sourceRange.start(), "wait");
        visitDefault(stmt);
    }

    void handle(const TimedStatement& stmt) {
        checkTiming(stmt.timing, stmt.sourceRange.start());
        visitDefault(stmt);
    }

private:
    void checkTiming(const TimingControl& timing, slang::SourceLocation loc) {
        if (timing.kind == TimingControlKind::SignalEvent) {
            check(timing.as<SignalEventControl>().expr, loc, "@");
        } else if (timing.kind == TimingControlKind::EventList) {
            // @(a or b) is guarded when any of its events is exempt, e.g. a clock edge.
            for (const auto* ev : timing.as<EventListControl>().events) {
                if (ev && ev->kind == TimingControlKind::SignalEvent) {
                    auto sig = vifSignal(ev->as<SignalEventControl>().expr);
                    if (!sig || sig->empty()) return;
                }
            }
            for (const auto* ev : timing.as<EventListControl>().events) {
                if (ev && ev->kind == TimingControlKind::SignalEvent) {
                    check(ev->as<SignalEventControl>().expr, loc, "@");
                    return;
                }
            }
        }
    }

    void check(const Expression& expr, slang::SourceLocation loc, std::string_view how) {
        if (guarded) return;
        auto sig = vifSignal(expr);
        if (!sig || sig->empty()) return;
        diags.addViolation(
            "SV-CON-015", "UnguardedBlockingWait",
            std::string(how) + " on interface signal '" + *sig + "' in '" + std::string(cls.name) +
                "' has no timeout. If the DUT never answers, the test hangs until the global "
                "timeout with no hint of which agent stalled. Race it against a delay in "
                "fork ... join_any, or call a wait-with-timeout helper.",
            core::Severity::Warning, loc);
    }

    const ClassType& cls;
    core::DiagnosticEngine& diags;
    int guarded = 0;
};

bool isTimeoutHelper(std::string_view name) {
    const std::string n = lower(name);
    return n.find("timeout") != std::string::npos || n.find("watchdog") != std::string::npos;
}

class UnguardedBlockingWait : public Rule {
public:
    std::string_view getId() const override { return "SV-CON-015"; }
    std::string_view getName() const override { return "UnguardedBlockingWait"; }
    std::string_view getDescription() const override {
        return "Flags wait(vif.sig) and @(vif.sig) in drivers and sequences that are not raced "
               "against a timeout in fork ... join_any. Clock, reset and clocking-block events are "
               "not flagged, nor are tasks whose name says timeout or watchdog. Experimental: a "
               "watchdog elsewhere in the testbench may already cover the wait.";
    }
    std::string_view getCategory() const override { return "Concurrency"; }
    core::Severity getDefaultSeverity() const override { return core::Severity::Warning; }
    RuleMaturity getMaturity() const override { return RuleMaturity::Experimental; }

    void run(slang::ast::Compilation&, const uvm::UvmModel& model, core::DiagnosticEngine& diags) override {
        for (const auto* cls : swe::userClasses(model)) {
            const auto role = swe::classRole(*cls);
            if (role != swe::Role::Driver && role != swe::Role::Sequence) continue;
            for (const auto* method : swe::classMethods(*cls)) {
                if (method->getParentScope() != cls || method->subroutineKind != SubroutineKind::Task) continue;
                if (isTimeoutHelper(method->name)) continue;
                UnguardedWaitWalker walker(*cls, diags);
                method->getBody().visit(walker);
            }
        }
    }
};

} // namespace uvmcheck::rules::concurrency

namespace uvmcheck::rules {
std::unique_ptr<Rule> createDisableNamedBlockInClass() {
    return std::make_unique<concurrency::DisableNamedBlockInClass>();
}
std::unique_ptr<Rule> createUnguardedBlockingWait() {
    return std::make_unique<concurrency::UnguardedBlockingWait>();
}
} // namespace uvmcheck::rules

UVMCHECK_REGISTER_RULE(uvmcheck::rules::createDisableNamedBlockInClass)
UVMCHECK_REGISTER_RULE(uvmcheck::rules::createUnguardedBlockingWait)
