#include "uvmcheck/engine/RuleEngine.h"

#include <algorithm>
#include <atomic>
#include <chrono>
#include <exception>
#include <iomanip>
#include <mutex>
#include <ostream>
#include <thread>
#include <tuple>

#include "uvmcheck/rules/Rule.h"

namespace uvmcheck::engine {

namespace {

using Clock = std::chrono::steady_clock;

double secondsSince(Clock::time_point start) {
    return std::chrono::duration<double>(Clock::now() - start).count();
}

/// Runs jobs on up to `threads` workers. Jobs are claimed in order, so put the
/// longest ones first. The first exception thrown by a job is rethrown here.
void parallelFor(std::vector<std::function<void()>>& jobs, unsigned threads) {
    if (jobs.empty()) return;
    unsigned workers = std::min<unsigned>(threads, static_cast<unsigned>(jobs.size()));
    if (workers <= 1) {
        for (auto& job : jobs) job();
        return;
    }

    std::atomic<size_t> next{0};
    std::exception_ptr error;
    std::mutex errorMutex;
    auto worker = [&] {
        for (size_t i = next++; i < jobs.size(); i = next++) {
            try {
                jobs[i]();
            }
            catch (...) {
                std::lock_guard lock(errorMutex);
                if (!error) error = std::current_exception();
            }
        }
    };

    std::vector<std::jthread> pool;
    pool.reserve(workers - 1);
    for (unsigned i = 1; i < workers; i++) pool.emplace_back(worker);
    worker();
    pool.clear(); // joins
    if (error) std::rethrow_exception(error);
}

/// Per-subscriber pruning state shared by both walkers. A subscriber whose
/// handler did not call visitDefault() on a node is muted for that node's
/// subtree, exactly as its own walk would have stopped there.
class MuteSet {
public:
    explicit MuteSet(size_t count) : depth(count, 0), active(count) {}

    bool isActive(uint16_t idx) const { return depth[idx] == 0; }
    bool anyActive() const { return active > 0; }
    size_t mark() const { return stack.size(); }

    void mute(uint16_t idx) {
        if (depth[idx]++ == 0) active--;
        stack.push_back(idx);
    }

    void restore(size_t mark) {
        while (stack.size() > mark) {
            if (--depth[stack.back()] == 0) active++;
            stack.pop_back();
        }
    }

private:
    std::vector<uint32_t> depth;
    std::vector<uint16_t> stack;
    size_t active;
};

/// One shared walk over every syntax tree for a group of subscribers.
class SyntaxWalker {
public:
    explicit SyntaxWalker(std::vector<SyntaxSubscriber*> subscribers) :
        subs(std::move(subscribers)), mutes(subs.size()) {
        for (uint16_t i = 0; i < subs.size(); i++) {
            if (subs[i]->wantsTokens()) tokenSubs.push_back(i);
        }
    }

    void walk(const slang::syntax::SyntaxNode& node) {
        const auto& interested = subscribersFor(node);
        size_t mark = mutes.mark();
        for (uint16_t idx : interested) {
            if (mutes.isActive(idx) && !subs[idx]->dispatch(node)) mutes.mute(idx);
        }

        if (mutes.anyActive()) {
            for (uint32_t i = 0; i < node.getChildCount(); i++) {
                if (auto child = node.childNode(i)) {
                    walk(*child);
                }
                else if (auto token = node.childToken(i)) {
                    for (uint16_t idx : tokenSubs) {
                        if (mutes.isActive(idx)) subs[idx]->token(token);
                    }
                }
            }
        }
        mutes.restore(mark);
    }

private:
    const std::vector<uint16_t>& subscribersFor(const slang::syntax::SyntaxNode& node) {
        size_t kind = static_cast<size_t>(node.kind);
        if (kind >= byKind.size()) {
            byKind.resize(kind + 1);
            probed.resize(kind + 1, false);
        }
        if (!probed[kind]) {
            probed[kind] = true;
            for (uint16_t i = 0; i < subs.size(); i++) {
                if (subs[i]->handles(node)) byKind[kind].push_back(i);
            }
        }
        return byKind[kind];
    }

    std::vector<SyntaxSubscriber*> subs;
    std::vector<uint16_t> tokenSubs;
    std::vector<std::vector<uint16_t>> byKind;
    std::vector<bool> probed;
    MuteSet mutes;
};

/// One shared walk over the elaborated AST (VisitFlags::AllGood order) for a
/// group of subscribers.
class AstWalker : public slang::ast::ASTVisitor<AstWalker, slang::ast::VisitFlags::AllGood> {
public:
    explicit AstWalker(std::vector<AstSubscriber*> subscribers) :
        subs(std::move(subscribers)), mutes(subs.size()) {}

    template<typename T>
    void handle(const T& node) {
        constexpr AstFamily family = astFamilyOf<T>();
        if constexpr (family == AstFamily::Count) {
            visitDefault(node);
        }
        else {
            const void* base = static_cast<const FamilyBase<family>*>(&node);
            const auto& interested = subscribersFor(family, static_cast<size_t>(node.kind), base);
            size_t mark = mutes.mark();
            for (uint16_t idx : interested) {
                if (mutes.isActive(idx) && !subs[idx]->dispatch(family, base)) mutes.mute(idx);
            }
            if (mutes.anyActive()) visitDefault(node);
            mutes.restore(mark);
        }
    }

private:
    template<AstFamily F>
    static auto familyBaseTag() {
        using namespace slang::ast;
        if constexpr (F == AstFamily::Symbol) return static_cast<Symbol*>(nullptr);
        else if constexpr (F == AstFamily::Statement) return static_cast<Statement*>(nullptr);
        else if constexpr (F == AstFamily::Expression) return static_cast<Expression*>(nullptr);
        else if constexpr (F == AstFamily::TimingControl) return static_cast<TimingControl*>(nullptr);
        else if constexpr (F == AstFamily::Constraint) return static_cast<Constraint*>(nullptr);
        else if constexpr (F == AstFamily::AssertionExpr) return static_cast<AssertionExpr*>(nullptr);
        else if constexpr (F == AstFamily::BinsSelectExpr) return static_cast<BinsSelectExpr*>(nullptr);
        else return static_cast<Pattern*>(nullptr);
    }
    template<AstFamily F>
    using FamilyBase = std::remove_pointer_t<decltype(familyBaseTag<F>())>;

    const std::vector<uint16_t>& subscribersFor(AstFamily family, size_t kind, const void* node) {
        auto& table = byKind[static_cast<size_t>(family)];
        auto& seen = probed[static_cast<size_t>(family)];
        if (kind >= table.size()) {
            table.resize(kind + 1);
            seen.resize(kind + 1, false);
        }
        if (!seen[kind]) {
            seen[kind] = true;
            for (uint16_t i = 0; i < subs.size(); i++) {
                if (subs[i]->handles(family, node)) table[kind].push_back(i);
            }
        }
        return table[kind];
    }

    std::vector<AstSubscriber*> subs;
    std::vector<std::vector<uint16_t>> byKind[size_t(AstFamily::Count)];
    std::vector<bool> probed[size_t(AstFamily::Count)];
    MuteSet mutes;
};

} // namespace

RuleEngine::RuleEngine(slang::ast::Compilation& compilation, const uvm::UvmModel& model,
                       const slang::SourceManager& sourceManager, EngineOptions options) :
    compilation(compilation), model(model), sourceManager(sourceManager), options(options) {}

void RuleEngine::addExternalJob(std::string name,
                                std::function<void(core::DiagnosticEngine&)> job) {
    externalJobs.emplace_back(std::move(name), std::move(job));
}

void RuleEngine::run(std::span<rules::Rule* const> rules, core::DiagnosticEngine& out) {
    auto runStart = Clock::now();
    const bool parallel = options.mode == ExecutionMode::SinglePass;
    unsigned threads = options.threads ? options.threads : std::thread::hardware_concurrency();
    threads = parallel ? std::max(1u, threads) : 1u;
    threadsUsed = threads;

    // Rules only read the design. Finish all lazy elaboration first and freeze
    // the compilation while rules run in parallel, the same protocol slang's
    // own multithreaded analysis uses.
    (void)compilation.getAllDiagnostics();
    if (parallel) compilation.freeze();
    struct Unfreeze {
        slang::ast::Compilation& c;
        bool active;
        ~Unfreeze() {
            if (active) c.unfreeze();
        }
    } unfreeze{compilation, parallel};

    std::vector<std::unique_ptr<RuleContext>> contexts;
    std::vector<RuleTask> tasks;
    contexts.reserve(rules.size());
    tasks.reserve(rules.size());
    for (auto* rule : rules) {
        contexts.push_back(
            std::make_unique<RuleContext>(compilation, model, sourceManager, options.mode));
        tasks.push_back(rule->check(*contexts.back()));
    }

    std::vector<core::DiagnosticEngine> externalDiags;
    externalDiags.reserve(externalJobs.size());
    for (size_t i = 0; i < externalJobs.size(); i++) externalDiags.emplace_back(sourceManager);

    std::mutex timingMutex;
    auto timed = [&](std::string label, auto&& fn) {
        if (!options.profile) {
            fn();
            return;
        }
        auto start = Clock::now();
        fn();
        double elapsed = secondsSince(start);
        std::lock_guard lock(timingMutex);
        timings.push_back({std::move(label), elapsed});
    };

    // Rules that are ready to run their body until the next suspension point.
    std::vector<size_t> ready(rules.size());
    for (size_t i = 0; i < ready.size(); i++) ready[i] = i;
    // Rules suspended waiting for a shared walk.
    std::vector<size_t> waiting;
    bool firstRound = true;

    while (!ready.empty() || !waiting.empty() || firstRound) {
        std::vector<std::function<void()>> jobs;

        // Shared walks for everything that is waiting, split into shards.
        std::vector<SyntaxSubscriber*> syntaxSubs;
        std::vector<AstSubscriber*> astSubs;
        for (size_t idx : waiting) {
            for (auto& s : contexts[idx]->pendingSyntax) syntaxSubs.push_back(s.get());
            for (auto& s : contexts[idx]->pendingAst) astSubs.push_back(s.get());
        }
        auto addShards = [&](auto& subs, auto makeJob) {
            size_t shards = std::min<size_t>(threads, subs.size());
            for (size_t shard = 0; shard < shards; shard++) {
                std::remove_reference_t<decltype(subs)> group;
                for (size_t i = shard; i < subs.size(); i += shards) group.push_back(subs[i]);
                jobs.push_back(makeJob(std::move(group), shard));
            }
        };
        addShards(astSubs, [&](std::vector<AstSubscriber*> group, size_t shard) {
            return std::function<void()>([&, group = std::move(group), shard]() mutable {
                timed("ast walk shard " + std::to_string(shard) + " (" +
                          std::to_string(group.size()) + " visitors)",
                      [&] {
                          AstWalker walker(std::move(group));
                          compilation.getRoot().visit(walker);
                      });
            });
        });
        addShards(syntaxSubs, [&](std::vector<SyntaxSubscriber*> group, size_t shard) {
            return std::function<void()>([&, group = std::move(group), shard]() mutable {
                timed("syntax walk shard " + std::to_string(shard) + " (" +
                          std::to_string(group.size()) + " visitors)",
                      [&] {
                          SyntaxWalker walker(std::move(group));
                          for (auto& tree : compilation.getSyntaxTrees()) {
                              if (tree) walker.walk(tree->root());
                          }
                      });
            });
        });

        // Rule bodies (standalone rules do all their work here).
        for (size_t idx : ready) {
            jobs.push_back([&, idx] {
                auto* ctx = contexts[idx].get();
                ctx->pendingSyntax.clear();
                ctx->pendingAst.clear();
                bool standalone = ctx->pendingStandalone;
                ctx->pendingStandalone = false;
                if (standalone) timed(std::string(rules[idx]->getId()), [&] { tasks[idx].resume(); });
                else tasks[idx].resume();
            });
        }

        if (firstRound) {
            // Python plugins and other external jobs only read the model, so
            // they run alongside everything else.
            for (size_t i = 0; i < externalJobs.size(); i++) {
                jobs.push_back([&, i] {
                    timed(externalJobs[i].first, [&] { externalJobs[i].second(externalDiags[i]); });
                });
            }
        }
        firstRound = false;

        parallelFor(jobs, threads);

        // Rules that were waiting on a walk are ready to continue; rules that
        // just ran are finished, waiting on a walk, or queued as standalone jobs.
        std::vector<size_t> nextReady = waiting;
        std::vector<size_t> nextWaiting;
        for (size_t idx : ready) {
            if (tasks[idx].done()) continue;
            auto* ctx = contexts[idx].get();
            if (ctx->pendingStandalone) nextReady.push_back(idx);
            else nextWaiting.push_back(idx);
        }
        std::sort(nextReady.begin(), nextReady.end());
        ready = std::move(nextReady);
        waiting = std::move(nextWaiting);
    }

    // Deterministic merge. Slang iterates class specializations in hash order,
    // so the order in which any walk meets findings can change from run to
    // run; sorting by location makes output stable and easy to read.
    auto& merged = out.getViolations();
    size_t firstNew = merged.size();
    for (auto& ctx : contexts) {
        auto& v = ctx->diags().getViolations();
        merged.insert(merged.end(), std::make_move_iterator(v.begin()),
                      std::make_move_iterator(v.end()));
    }
    for (auto& d : externalDiags) {
        auto& v = d.getViolations();
        merged.insert(merged.end(), std::make_move_iterator(v.begin()),
                      std::make_move_iterator(v.end()));
    }
    std::stable_sort(merged.begin() + static_cast<std::ptrdiff_t>(firstNew), merged.end(),
                     [](const core::Violation& a, const core::Violation& b) {
                         return std::tie(a.filePath, a.line, a.column, a.ruleId, a.message) <
                                std::tie(b.filePath, b.line, b.column, b.ruleId, b.message);
                     });
    totalSeconds = secondsSince(runStart);
}

void RuleEngine::printProfile(std::ostream& os) const {
    auto sorted = timings;
    std::sort(sorted.begin(), sorted.end(),
              [](const Timing& a, const Timing& b) { return a.seconds > b.seconds; });
    os << "[profile] rule engine: " << std::fixed << std::setprecision(3) << totalSeconds
       << " s wall on " << threadsUsed << " thread(s)\n";
    for (const auto& t : sorted) {
        os << "[profile]   " << std::setw(8) << std::setprecision(4) << t.seconds << " s  "
           << t.label << "\n";
    }
}

} // namespace uvmcheck::engine
