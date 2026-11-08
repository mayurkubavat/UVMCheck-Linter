#pragma once

#include <functional>
#include <iosfwd>
#include <span>
#include <string>
#include <vector>

#include "uvmcheck/engine/RuleContext.h"

namespace uvmcheck::rules {
class Rule;
}

namespace uvmcheck::engine {

struct EngineOptions {
    ExecutionMode mode = ExecutionMode::SinglePass;
    /// Worker threads; 0 picks std::thread::hardware_concurrency().
    unsigned threads = 0;
    /// Collect per-job timings for printProfile().
    bool profile = false;
};

/// Runs a set of rules against one elaborated compilation.
///
/// Execution model (SinglePass):
///  1. Every rule's check() coroutine is started. Rules that hand visitors to
///     ctx.visitSyntax()/visitAst() suspend; rules that walk on their own
///     yield once and become independent jobs.
///  2. The visitors are split into shards (one per worker). Each shard does a
///     single syntax walk and a single AST walk, routing every node only to
///     the visitors that have a handle() for its type. Shards and standalone
///     rules run concurrently on the worker pool.
///  3. Suspended rules resume for post-walk reporting, possibly asking for
///     another walk, until every rule finishes.
///
/// Each rule writes to its own DiagnosticEngine; results are merged in rule
/// registration order, so output is identical regardless of thread count.
class RuleEngine {
public:
    RuleEngine(slang::ast::Compilation& compilation, const uvm::UvmModel& model,
               const slang::SourceManager& sourceManager, EngineOptions options);

    /// Extra work (e.g. the Python plugin runner) that runs concurrently with
    /// the rules. Its findings are appended after all rule findings.
    void addExternalJob(std::string name, std::function<void(core::DiagnosticEngine&)> job);

    /// Runs the rules and appends their violations to `out`.
    void run(std::span<rules::Rule* const> rules, core::DiagnosticEngine& out);

    void printProfile(std::ostream& os) const;

private:
    struct Timing {
        std::string label;
        double seconds;
    };

    slang::ast::Compilation& compilation;
    const uvm::UvmModel& model;
    const slang::SourceManager& sourceManager;
    EngineOptions options;
    std::vector<std::pair<std::string, std::function<void(core::DiagnosticEngine&)>>> externalJobs;
    std::vector<Timing> timings;
    double totalSeconds = 0;
    unsigned threadsUsed = 1;
};

} // namespace uvmcheck::engine
