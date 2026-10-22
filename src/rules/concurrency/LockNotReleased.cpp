#include "uvmcheck/rules/Rule.h"
#include "uvmcheck/rules/RuleRegistry.h"
#include "uvmcheck/rules/PathWalker.h"
#include "uvmcheck/rules/SweSupport.h"
#include "slang/ast/expressions/OperatorExpressions.h"

#include <map>
#include <set>

namespace uvmcheck::rules::concurrency {

using namespace slang::ast;

namespace {

enum class LockOp { None, Acquire, Release };

// Classifies a call as taking or giving back a lock, and names the lock it works on.
LockOp classify(const CallExpression& call, std::string& key) {
    std::string_view n = call.getSubroutineName();
    const Expression* recv = call.thisClass();
    bool isSemaphore = false;
    if (recv && recv->type) {
        if (const auto* cls = swe::asClass(*recv->type)) isSemaphore = cls->name == "semaphore";
    }
    if (isSemaphore) {
        key = swe::exprText(*recv);
        if (n == "get") return LockOp::Acquire;
        if (n == "put") return LockOp::Release;
        return LockOp::None;
    }
    // Sequencer arbitration locks, taken from a sequence or through the sequencer handle.
    if (n == "lock" || n == "grab") { key = std::string(n); return LockOp::Acquire; }
    if (n == "unlock") { key = "lock"; return LockOp::Release; }
    if (n == "ungrab") { key = "grab"; return LockOp::Release; }
    return LockOp::None;
}

struct Held {
    std::map<std::string, int> keys;
    bool operator==(const Held& o) const { return keys == o.keys; }
};

struct Hooks {
    const SubroutineSymbol& sub;
    std::set<std::string> released; // keys this routine gives back somewhere
    std::map<std::string, slang::SourceLocation> acquiredAt;
    std::vector<std::pair<std::string, slang::SourceLocation>> leaks;
    std::set<std::string> reported;

    void apply(const CallExpression& call, Held& st) {
        std::string key;
        switch (classify(call, key)) {
            case LockOp::Acquire:
                st.keys[key]++;
                acquiredAt.emplace(key, swe::locOf(call));
                break;
            case LockOp::Release:
                if (auto it = st.keys.find(key); it != st.keys.end() && --it->second <= 0) st.keys.erase(it);
                break;
            default:
                break;
        }
    }

    void leaf(const Statement& s, Held& st) {
        swe::forEachCall(s, [&](const CallExpression& call) { apply(call, st); });
    }

    void branch(const Expression& cond, bool taken, Held& st) {
        // if (sem.try_get()) ... holds the key only on the taken side.
        const Expression* e = &swe::stripConversions(cond);
        bool negated = false;
        if (e->kind == ExpressionKind::UnaryOp && e->as<UnaryExpression>().op == UnaryOperator::LogicalNot) {
            negated = true;
            e = &swe::stripConversions(e->as<UnaryExpression>().operand());
        }
        if (e->kind != ExpressionKind::Call) return;
        const auto& call = e->as<CallExpression>();
        if (call.getSubroutineName() != "try_get" || !call.thisClass()) return;
        if (taken != negated) {
            std::string key = swe::exprText(*call.thisClass());
            st.keys[key]++;
            acquiredAt.emplace(key, swe::locOf(call));
        }
    }

    void returned(const Statement& ret, const Held& st) { leak(st, swe::locOf(ret)); }

    void leak(const Held& st, slang::SourceLocation at) {
        for (const auto& [key, count] : st.keys) {
            if (count <= 0 || !released.count(key) || reported.count(key)) continue;
            reported.insert(key);
            leaks.push_back({key, at});
        }
    }
};

} // namespace

class LockNotReleased : public Rule {
public:
    std::string_view getId() const override { return "SV-RSC-001"; }
    std::string_view getName() const override { return "LockNotReleased"; }
    std::string_view getDescription() const override {
        return "Flags semaphore get()/try_get() and sequencer lock()/grab() that some path leaves without "
               "the matching put()/unlock()/ungrab(), such as an early return (Sonar S2222).";
    }
    std::string_view getCategory() const override { return "Concurrency"; }
    core::Severity getDefaultSeverity() const override { return core::Severity::Error; }

    void run(Compilation& compilation, const uvm::UvmModel& model, core::DiagnosticEngine& diags) override {
        for (const auto* sub : swe::allUserRoutines(compilation, model)) {
            Hooks hooks{*sub, {}, {}, {}, {}};
            // Only routines that release a lock themselves are expected to release it on every path;
            // acquire-only helpers hand the lock to their caller on purpose.
            swe::forEachCall(sub->getBody(), [&](const CallExpression& call) {
                std::string key;
                if (classify(call, key) == LockOp::Release) hooks.released.insert(key);
            });
            if (hooks.released.empty()) continue;

            swe::PathWalker<Held, Hooks> walker(hooks);
            for (const auto& st : walker.walkBody(sub->getBody(), Held{})) hooks.leak(st, sub->location);

            for (const auto& [key, at] : hooks.leaks) {
                bool atEnd = at == sub->location;
                diags.addViolation(
                    "SV-RSC-001", "LockNotReleased",
                    "'" + key + "' is acquired at line " +
                        std::to_string(swe::lineOf(compilation, hooks.acquiredAt[key])) + " but " +
                        (atEnd ? std::string("some path through '") + std::string(sub->name) +
                                     "' ends without releasing it"
                               : std::string("this return leaves '") + std::string(sub->name) +
                                     "' while still holding it") +
                        ". The next requester then waits forever. Release it before every exit.",
                    core::Severity::Error, at);
            }
        }
    }
};

} // namespace uvmcheck::rules::concurrency

namespace uvmcheck::rules {
std::unique_ptr<Rule> createLockNotReleased() {
    return std::make_unique<concurrency::LockNotReleased>();
}
}
