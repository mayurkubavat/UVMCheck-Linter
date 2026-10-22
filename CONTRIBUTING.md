# Contributing to UVMCheck

Thank you for your interest in contributing to **UVMCheck**! We welcome bug reports, feature suggestions, architectural improvements, and new verification rules.

---

## Code of Conduct

Please be respectful, constructive, and collaborative in all issues, pull requests, and discussions.

---

## Development Environment Setup

### Prerequisites
- **Compiler:** Modern C++20 compiler (`clang >= 15`, `AppleClang >= 15`, or `gcc >= 12`)
- **Build System:** CMake `>= 3.28` and Ninja (`ninja-build` or `brew install ninja`)
- **Acceleration:** `ccache` (strongly recommended)
- **Scripting:** Python 3.9+ (for the regression test runner)

### Memory-Bounded Build Workflow
UVMCheck is configured with CMake presets and Ninja job pools to prevent out-of-memory compiler spikes when building heavy AST visitors:

```bash
# Clone the repository
git clone https://github.com/<your-org>/UVMCheck-Linter.git
cd UVMCheck-Linter

# Configure using Release preset
cmake --preset default

# Build with bounded concurrency (peak RAM stays < 1.8 GB)
cmake --build build -j 4
```

---

## Adding a New Verification Rule

UVMCheck's architecture makes it straightforward to add new rules without modifying core engine logic.

### 1. Choose Rule Category & ID
Locate the relevant category directory under `src/rules/`:
- `concurrency/` - Race conditions, clocking blocks, fork-join pitfalls
- `correctness/` - Timescales, bitwidth mismatches, off-by-one errors
- `coverage/` - Covergroup syntax, bin limits, illegal bins
- `deprecated/` - Legacy OVM / banned UVM APIs
- `oop/` - Virtual methods, downcasting, constructors
- `performance/` - Unbounded dynamic array copies, string bottlenecks
- `random/` - Unchecked `randomize()`, constraint solving issues
- `sva/` - Vacuous assertions, combinatorial triggers
- `uvm/` - Factory, phasing, objections, `uvm_config_db`, sequences
- `errors/` - Failures that are not reported, checkers that cannot fail, use after a failed check
- `design/` - Testbench layers, hierarchy reach-through, god class, cohesion, coupling, overrides
- `metrics/` - Cognitive complexity, routine length, nesting, argument count, class size
- `readability/` - Flag arguments, const ref, commented-out code, magic numbers

Rules in `errors/`, `design/`, `metrics/` and `readability/` (and the SV-BUG, SV-RSC, SV-CNC-003/004 and
SV-THR-003 rules) build on shared helpers instead of re-implementing them:

- `uvmcheck/rules/SweSupport.h`: UVM role of a class (driver, monitor, scoreboard, env, test, config,
  register model ...), user classes and methods with the UVM library filtered out, `uvm_*` report macro
  handling for both the test stub and real uvm-core (`isReportGuard`, `reportLevel`, `alwaysExits`), written
  and referenced symbols, and `UserCodeVisitor`, a visitor root that skips the UVM library.
- `uvmcheck/rules/CodeMetrics.h`: per-routine metrics (cognitive and cyclomatic complexity, nesting,
  statements) and per-class metrics (WMC, ATFD, TCC, LCOM4, CBO, DIT), computed once per compilation.
- `uvmcheck/rules/PathWalker.h`: a small path-sensitive walker for rules that track state along paths.

Thresholds go through `swe::ruleOption("<RULE-ID>", "<key>", default)` so the project config can override
them per rule. Opinionated rules use the category `Advisory`: they are off unless `--preset advisory`,
`--preset all` or `--rule` selects them, in the CLI and in the language server.

Assign a unique rule ID conforming to the standard naming scheme (e.g. `UVM-XYZ-001`, `SV-PRF-013`).

### 2. Implement the Rule Class
Create `src/rules/<category>/YourRule.cpp`. Rules hand their visitor to the
engine's shared walk instead of walking the design themselves:

```cpp
#include "uvmcheck/rules/Rule.h"
#include "uvmcheck/rules/RuleRegistry.h"

namespace uvmcheck::rules::uvm {

// Derive from engine::AstVisitor (elaborated AST) or engine::SyntaxVisitor
// (syntax trees). They are drop-in replacements for slang's visitors.
class YourRuleVisitor : public engine::AstVisitor<YourRuleVisitor> {
public:
    explicit YourRuleVisitor(core::DiagnosticEngine& diags) : diags(diags) {}

    void handle(const slang::ast::NewClassExpression& expr) {
        // ... report with diags.addViolation(...) ...
        visitDefault(expr); // descend into children (omit to prune the subtree)
    }

private:
    core::DiagnosticEngine& diags;
};

class YourRule : public Rule {
public:
    std::string_view getId() const override { return "UVM-XYZ-001"; }
    std::string_view getName() const override { return "YourRuleName"; }
    std::string_view getDescription() const override { return "What this rule detects and why."; }
    std::string_view getCategory() const override { return "UVM"; }
    core::Severity getDefaultSeverity() const override { return core::Severity::Warning; }

    engine::RuleTask check(engine::RuleContext& ctx) override {
        YourRuleVisitor visitor(ctx.diags());
        co_await ctx.visitAst(visitor); // or ctx.visitSyntax(visitor)
        // Code here runs after the walk (e.g. reporting collected state).
    }
};

} // namespace uvmcheck::rules::uvm
```

**How the engine runs it.** `co_await ctx.visitAst(v)` suspends the rule; the
engine then does one walk per shard and routes each node only to visitors that
have a `handle()` for its type, on a worker pool. Shared-walk handlers must
follow two rules:

- Call `visitDefault(node)` as the **last** statement of a handler (or not at
  all, to prune). Code after it would run *before* the children are visited.
  If a handler needs enter/exit logic around its children (depth counters,
  scope stacks), visit the children yourself inside the handler, or override
  `run()` instead of `check()` to get a private walk.
- Only read the design and write to your own `diags`. Rules run in parallel
  and each has its own `RuleContext`.

Rules that need a custom traversal can still override
`run(compilation, uvmModel, diags)`; they run as independent parallel jobs.
`tests/engine_equivalence.py` checks that both styles give the same findings
as the reference `--engine isolated` mode.

### 3. Register the Rule
Add your rule factory to `src/rules/BuiltinRules.cpp`.

### 4. Create Paired Unit Tests
Every rule in UVMCheck **must** have paired `bad.sv` and `good.sv` test cases plus an `expect.json`:
1. Create directory `tests/rules/<category>/<RULE_ID>/`.
2. Add `bad.sv` containing an isolated violation that should trigger the rule.
3. Add `good.sv` containing the compliant implementation that should pass cleanly without warnings.
4. Add `expect.json` listing every finding `bad.sv` must produce, by rule and line:
   ```json
   { "bad.sv": [ { "rule": "UVM-XYZ-001", "line": 17 } ] }
   ```
   `python3 tests/regression.py --rule <RULE_ID> --update-expectations` writes it from the
   current output. Check every line it records before committing: it captures what the
   rule does, not what it should do.

### 5. Verify via Regression Suite
The runner reads findings from the linter's `--json` report and compile errors from
slang's `--diag-json`. It has three suites, all of which run in CI:

```bash
# 1. Isolated: only the rule under test, against the UVM stub (tests/common/uvm_mock.svh)
python3 tests/regression.py --rule <RULE_ID>

# 2. All rules enabled, against the UVM stub
python3 tests/regression.py --mode all-rules --baseline tests/baselines/all_rules_stub.json

# 3. All rules enabled, against the real Accellera library
git clone --depth 1 --branch 2020.3.1 https://github.com/accellera-official/uvm-core.git ../uvm-core
python3 tests/regression.py --mode all-rules --uvm-home ../uvm-core \
    --baseline tests/baselines/all_rules_uvm_core.json

# Everything via CTest (suite 3 only when UVMCHECK_UVM_HOME is set at configure time)
cmake --preset default -DUVMCHECK_UVM_HOME=$PWD/../uvm-core
ctest --test-dir build --output-on-failure
```

In the all-rules suites, findings from *other* rules in `bad.sv`/`good.sv`, and compile
errors (as `COMPILE:<code>@<line>`), must be listed in the suite's baseline file. A new one fails the run, and so does a baseline entry that no
longer fires, so the baseline only shrinks. After fixing a false positive, refresh it with
`--update-baseline` and commit the smaller file.

---

## Pull Request Guidelines

1. **Commit Hygiene**: Ensure code compiles warning-free on Clang and GCC.
2. **Zero Regressions**: All 450+ unit tests must pass before opening a PR.
3. **Documentation**: Update the [Rule Catalog](references/rules/catalog/uvmcheck_rule_catalog.md) when introducing new rules or modifying rule semantics.
4. **Automated Fixes**: If your rule supports `--fix`, include test cases verifying the exact textual rewrite output.
