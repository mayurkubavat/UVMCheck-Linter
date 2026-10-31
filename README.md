# UVMCheck - High-Performance SystemVerilog & UVM Linter

[![C++20](https://img.shields.io/badge/Language-C%2B%2B20-blue.svg)](https://en.cppreference.com/w/cpp/20)
[![Slang](https://img.shields.io/badge/Powered%20By-Slang-orange.svg)](https://github.com/MikePopoloski/slang)
[![Tests](https://img.shields.io/badge/Tests-456%2F456%20Passing-brightgreen.svg)](tests/regression.py)
[![License](https://img.shields.io/badge/License-MIT-green.svg)](LICENSE)

**UVMCheck** is a blazing-fast, static analysis and linting engine designed specifically for modern **IEEE 1800 (SystemVerilog)** and **IEEE 1800.2 (UVM)** design verification codebases. Built on modern C++20 and leveraging the high-speed Slang compiler frontend, UVMCheck provides deep semantic AST and CST analysis, full factory and phasing validation, simulation-race detection, and automated fix-it capabilities.

---

## Key Features

- **228+ Specialized Verification Rules**: Comprehensive coverage across 18 functional domains (UVM methodology, OOP design, concurrency & delta-cycle races, SVA assertion health, coverage bins, random constraints, and code style).
- **Deep Semantic Graph Analyzers**:
  - **Objection Control Flow Graph (CFG)**: Verifies every `raise_objection()` path strictly reaches a matching `drop_objection()` without early-return leaks or asymmetric branch divergence.
  - **Config DB Cross-Component Resolver**: Matches `set()` and `get()` calls across hierarchy strings to detect type mismatches and unconfigured lookups.
  - **TLM Port-to-Export Elaboration Checker**: Detects unmapped driver pull ports and unconnected analysis ports across `connect_phase()` methods.
- **Sub-Second Execution**: Analyzes complex verification environments at rates exceeding 20,000 lines per second.
- **Type-Safe Semantic Analysis**: Unlike regex-based linters, UVMCheck resolves complete symbol hierarchies, parameterizations, and virtual method tables using Slang's typed AST.
- **Memory-Bounded Compilation**: Optimized build architecture with Ninja job pools, precompiled headers (PCH), and `ccache` keeping peak compile RAM strictly under 1.8 GB on Apple Silicon and Linux dev boxes.
- **Automated Fix-It Engine (`--fix`)**: Applies automated, zero-error syntactical refactorings (e.g. adding missing end-labels, fixing macro mismatches).
- **Baseline Technical Debt Management (`--baseline`)**: Snapshot legacy violations into a JSON baseline to enforce zero-regression on new pull requests.
- **Granular Waivers (`--waivers`)**: Waive rules by rule ID, file glob pattern, or line number with mandatory rationale tracking.
- **CI/CD Integration**: Emits industry-standard SARIF 2.1.0 (for GitHub Code Scanning / GitLab Code Quality) and interactive, self-contained HTML dashboards.
- **Modular Unit Testsuite**: 238 per-rule test directories with 476 paired `bad.sv` and `good.sv` test cases running in ~3 seconds with 100% pass rate.

---

## Rule Domain Categories

| Category | Rules | Description |
| :--- | :---: | :--- |
| **UVM Methodology** | 53 | Factory registration, phasing, objection CFG balancing, cross-component config_db, TLM topology |
| **Concurrency** | 13 | Delta-cycle oscillations, `always_comb` triggers, fork-join variable capture, circular wait deadlocks |
| **SVA (Assertions)** | 14 | Vacuous antecedents, combinatorial assert glitches, disable-iff resets, multi-clock domains |
| **Randomization** | 18 | Unchecked `randomize()`, non-pure functions in constraints, cyclic `solve-before`, randc exhaustion |
| **Coverage** | 16 | Cross-bin explosion, illegal bin overlaps, 0-weight coverpoints, unreferenced covergroups |
| **OOP & Architecture** | 22 | Non-virtual overrides, object slicing, downcast safety, virtual call in constructor |
| **Performance** | 18 | Deep-copying dynamic arrays by value, string concatenation in loops, unbounded arrays in constraints |
| **Correctness & Safety** | 30 | Off-by-one queue indexing, self-comparisons, non-blocking assignments in functions, casex hazards |
| **Types & DPI** | 15 | Mixed-sign comparisons, redundant `$cast`, non-void task return types, DPI width mismatches |
| **Deprecated & Smells** | 18 | OVM legacy APIs, banned `uvm_field_*` macros, `uvm_do` hiding, god-methods, magic delays |
| **Style & Formatting** | 21 | Mandatory end labels, line length limits, capitalization, naming conventions |
| **Software Engineering** | 47 | Copy-paste bugs, failures that are not reported, checkers that cannot fail, testbench layer rules, god classes, cohesion and coupling metrics, cognitive complexity, lock and file-handle leaks |

> Every rule, with its category, severity, presets, guidelines and links to its bad/good examples, is listed in the generated [Rule Catalog](docs/rules/README.md) (`uvmcheck --list-rules --json -` gives the same data as JSON).

---

## Getting Started

### Prerequisites
- Modern C++20 compiler (`clang >= 15`, `AppleClang >= 15`, or `gcc >= 12`)
- CMake `>= 3.28`
- Ninja (`brew install ninja` or `apt-get install ninja-build`)
- `ccache` (strongly recommended to accelerate rebuilds)
- Python 3.9+ (for test runner)

### Building UVMCheck

UVMCheck provides pre-configured CMake presets for fast, out-of-the-box building:

```bash
# Clone the repository
git clone https://github.com/mayurkubavat/UVMCheck-Linter.git
cd UVMCheck-Linter

# Configure using Release preset
cmake --preset default

# Build binary (peak RAM strictly bounded < 1.8 GB)
cmake --build build -j 4
```

The compiled binary will be located at `build/uvmcheck`.

---

## Running the Linter

### Basic Lint
```bash
# Lint a SystemVerilog source file with include directory
./build/uvmcheck path/to/my_env.sv -I path/to/includes

# Lint a package with multiple source files
./build/uvmcheck tb/top_pkg.sv tb/my_agent.sv tb/my_scoreboard.sv
```

### Rule Profiles & Presets
Filter rule execution using curated verification profiles:
```bash
# Run only soundness checks (concurrency, SVA, correctness, types)
./build/uvmcheck tb/top.sv --preset soundness

# Run only UVM methodology checks
./build/uvmcheck tb/top.sv --preset methodology

# Run only performance optimizations
./build/uvmcheck tb/top.sv --preset performance

# Run only code style checks
./build/uvmcheck tb/top.sv --preset style

# Run only maintainability checks (metrics, design smells)
./build/uvmcheck tb/top.sv --preset maintainability

# Run the opt-in advisory rules (magic numbers, commented-out code, ...)
./build/uvmcheck tb/top.sv --preset advisory

# Run everything, advisory rules included
./build/uvmcheck tb/top.sv --preset all
```

Without `--preset`, every rule except the Advisory ones runs. `--rule <ID>` always runs the named rule.

### Methodology Guide

The [UVM Methodology Guide](docs/methodology/README.md) groups guidelines for reusable, maintainable
testbenches into nine chapters (layering, coupling, component roles, reuse, design patterns, construct
usage, duplication, failure handling, evolvability), each backed by lint rules:

```bash
./build/uvmcheck --list-guidelines                 # the guide and the rules behind each guideline
./build/uvmcheck tb/top.sv --guideline MG-4         # one chapter (or MG-4.1 for one guideline)
./build/uvmcheck tb/top.sv --preset reuse           # chapters MG-1 to MG-5
./build/uvmcheck tb/top.sv --conformance            # pass/fail per guideline after any run
./build/uvmcheck tb/top.sv --guideline MG-9.2 --history  # hotspots and change coupling from git
```

### Single-Rule Targeting
Run or debug a specific rule against a source file in milliseconds:
```bash
./build/uvmcheck tb/my_seq.sv --rule UVM-BAN-001
./build/uvmcheck tb/my_scb.sv --rule ScoreboardLeftoverQueue
```

### Compiler errors and warnings
The compiler's own diagnostics are findings too: errors are `SV-CMP-001` and warnings `SV-CMP-002`. They appear in the console, JSON, SARIF, HTML and the editor, and can be waived or baselined like any other finding. `-W` options and `` `pragma diagnostic `` decide which warnings are reported and at what severity (`-Wno-redef-macro`, `-Werror=index-oob`). Disable the rules to get slang's plain diagnostic output back.

### Performance & Threading
Rules run on a single-pass, parallel engine: most rules hand their visitors to one
shared walk per shard, which routes each syntax/AST node only to the rules that
handle it, and shards run on a worker pool alongside the remaining standalone rules.
Findings are printed in file/line order and are identical for any thread count.
```bash
# Use 8 worker threads (default: all cores)
./build/uvmcheck tb/top.sv -j 8

# Show where time goes (elaboration, walk shards, slowest standalone rules)
./build/uvmcheck tb/top.sv --profile-rules

# Reference mode: one walk per rule, sequential (for debugging rule behavior)
./build/uvmcheck tb/top.sv --engine isolated
```
On the 86k-line Accellera `uvm-core` library, a full all-rules run takes about
0.5 s on 4 cores (previously about 2.9 s on the same machine).

### Automated Fix-It Rewrites
Some findings carry a mechanical rewrite (`casex` to `casez`, `== 'x` to `===`, ...). Preview them as a unified diff, then apply:
```bash
./build/uvmcheck tb/legacy_code.sv --fix-dry-run   # print the diff, change nothing
./build/uvmcheck tb/legacy_code.sv --fix           # rewrite the files
./build/uvmcheck tb/legacy_code.sv --fix --fix-rule SV-COR-009,SV-STY-005   # only these rules' fixes
```
Fixes land in the file that holds the code, including included `.svh` files. Waived and baselined findings are not fixed. A file edited after it was read is skipped, fixes inside macro expansions are left alone, and when two fixes overlap the second waits for the next `--fix` run. Each file is written to a temporary copy and renamed into place.

### Reporting (SARIF, JSON & HTML)
```bash
# Machine-readable findings for scripts and CI ('-' writes to stdout)
./build/uvmcheck tb/*.sv --json results.json

# Generate SARIF 2.1.0 for CI/CD code scanning
./build/uvmcheck tb/*.sv --sarif results.sarif

# Generate interactive HTML dashboard
./build/uvmcheck tb/*.sv --html report.html
```

### Comparing and merging reports
`--json` reports carry a fingerprint per finding that survives edits elsewhere in the file, so runs can be compared:
```bash
python3 -m uvmcheck.report diff  main.json branch.json --fail-on-new     # new and fixed findings
python3 -m uvmcheck.report merge blk_a.json blk_b.json -o all.json       # one report from several jobs
python3 -m uvmcheck.report trend w1.json w2.json w3.json -o trend.html   # burn-down by category, guideline, block, rule
```
Run them from `python/`, or `pip install ./python` and use `uvmcheck-report`. JSON findings also name their rule's `category`.

### Custom Python Rules Plugin SDK
Verification teams can author custom corporate or project verification checks in pure Python without recompiling C++:
```bash
# Execute custom Python rules from a file or directory
./build/uvmcheck tb/*.sv --python-rules examples/python_rules/custom_naming_rules.py

# Python rule violations seamlessly participate in SARIF, HTML, and baselines
./build/uvmcheck tb/*.sv --python-rules my_rules/ --sarif results.sarif
```
See [Python Rule SDK Documentation](docs/python_rule_sdk.md) for full API reference, context query helpers, and tutorials.

### Explain a rule
`--explain SV-DUP-002` (ID or name) prints what the rule checks, the methodology guidelines
it enforces, the options it reads with their defaults, and a flagged and a fixed example.
The examples are the rule's regression tests (`tests/rules/<category>/<ID>/`), compiled into
the binary at build time, so they always match what the rule does.

### Testbench map
`--map tb_map.html` draws the testbench as the linter sees it: the component tree from the
test down (which class creates which), each component's TLM ports and `connect()` calls,
and the config_db fields set and read, with ports that are never connected, gets without a
set and sets nobody reads marked. Findings are attached to the component class they fall
in. Use a `.json` path for the same data in machine-readable form.

### Language Server Protocol (LSP) & IDE Integration
Run UVMCheck as an in-editor daemon for live, incremental linting in VS Code and Cursor:
```bash
# Launch LSP JSON-RPC daemon over stdin/stdout
./build/uvmcheck --lsp
```
The server finds the project config from the workspace root and compiles the
project the way the CLI does, with your unsaved buffers in place of the files on
disk. Give the config `sources` (or `filelists`), `include_dirs` and `defines`
so a class in an included `.svh` is linted with the UVM library and the rest of
the testbench around it; without them, the open files are compiled on their own.
Each finding carries its rule ID and a precise range, and offers a "Waive RULE on
this line" quick fix (plus the rule's own fix where it has one).

```python
class Project(ProjectConfig):
    sources = ["$UVM_HOME/src/uvm_pkg.sv", "tb/tb_pkg.sv", "tb/tb_top.sv"]
    include_dirs = ["$UVM_HOME/src", "tb"]
    defines = ["UVM_NO_DPI"]
```

See [`editors/vscode/`](editors/vscode/) for the ready-to-use VS Code / Cursor extension.

---

## Project Configuration

On an existing testbench, start with `uvmcheck --init` in its root folder. It
finds the sources (or a `.f` file list) and the UVM library, learns the naming
conventions your code already follows (for example drivers ending in `_bfm`
rather than `_driver`), writes `.uvmcheck/config.py`, and records today's
findings in `.uvmcheck/baseline.json` so the next run shows only new ones.
Review the generated config; it is ordinary Python you can edit.

Describe your project once in `.uvmcheck/config.py` or `uvmcheck_config.py` (auto-discovered from the working directory upward):

```python
from uvmcheck.config import ProjectConfig, RuleConfig, Waiver, Severity

class Project(ProjectConfig):
    library_paths = ["$UVM_HOME", "vip/**"]   # compiled, never reported
    exclude_paths = ["**/generated/**"]

    class MaxLineLength(RuleConfig):
        max_columns = 140

    class NoCasex(RuleConfig):
        rule = "SV-COR-009"
        severity = Severity.Error

    waivers = [Waiver("UVM-OBJ-002", path="tb/stress_seq.sv", line=142, reason="Stress bench")]
```

Linting a testbench against the real Accellera UVM library with `library_paths = ["$UVM_HOME"]` drops the ~18,000
findings inside UVM itself, including code generated by UVM macros in your classes, and leaves only your own code.
Teams can subclass a shared base config and override single rules. See [Project Configuration](docs/project_config.md)
for globs, rule options, the JSON form, and `--library` / `--exclude` flags.

## Baseline & Waiver Management

### Adopting on Existing Legacy Codebases
Establish a zero-new-defects baseline without fixing thousands of pre-existing legacy violations:

```bash
# 1. Snapshot all current legacy violations into baseline JSON
./build/uvmcheck tb/*.sv --update-baseline .uvmcheck/baseline.json

# 2. Enforce zero regressions on new PRs (only new violations fail CI)
# Note: .uvmcheck/baseline.json is auto-discovered if present!
./build/uvmcheck tb/*.sv
```

Baseline entries are matched by a fingerprint of the rule, the file's path relative to the
project root (the folder holding `.uvmcheck/`) and the text of the flagged line, so adding or
removing lines elsewhere in a file, or checking the project out somewhere else, does not
bring old findings back. Editing the flagged line does: that finding is treated as touched
and must be fixed or re-baselined. Entries that no longer match anything are reported so the
baseline only shrinks; `--update-baseline` drops them.

Inline waivers go on the flagged line (`// uvmcheck:waive SV-COR-008 reason`) or on the line
above it (`// uvmcheck-disable-next-line SV-COR-008`).

### Waivers Configuration
Document approved exceptions in `.uvmcheck/waivers.json` (auto-discovered) or in your `config.py`:
```json
{
  "version": "1.0",
  "waivers": [
    {
      "rule": "SV-STY-002",
      "path": "*legacy_block*",
      "reason": "Permit trailing semicolons in legacy third-party macros"
    },
    {
      "rule": "UVM-OBJ-002",
      "file": "tb/stress_seq.sv",
      "line": 142,
      "reason": "Objection in loop verified for stress bench iteration",
      "owner": "mayur"
    }
  ]
}
```
Run with waivers automatically discovered:
```bash
./build/uvmcheck tb/*.sv
```
(Or pass `--waivers path/to/waivers.json` explicitly).

### New code is clean, without a baseline
`--changed-since <git-ref>` reports only findings on lines added or changed since that
revision, plus everything in untracked files. In a pull request job:

```bash
./build/uvmcheck --changed-since origin/main
```

### Keeping suppressions honest
Waivers and baselines pile up unless something prunes them. `--waiver-report` lists
waivers that matched nothing in this run, expired waivers, waivers without an owner,
waiver comments in your sources that no longer waive anything, baseline entries whose
finding is gone, and how many findings are waived per rule and per owner.
`--strict-waivers` prints the same and exits with status 4 while any unused, expired or
stale entry remains, so CI keeps the suppression list as short as the code allows:

```bash
./build/uvmcheck --strict-waivers   # in CI, after the normal lint step
```

---

## Regression & Unit Testsuite

UVMCheck features a modular unit test suite with 648 test cases covering 324 rules:

```bash
# Run entire testsuite via Python runner (~2.8 seconds)
python3 tests/regression.py

# Run only UVM methodology tests (~0.5 seconds)
python3 tests/regression.py --category uvm

# Run single-rule test (~7 ms)
python3 tests/regression.py --rule UVM-CFG-002

# Check the parallel engine gives identical findings to the reference engine
python3 tests/engine_equivalence.py

# Git-history rules (SV-EVO-001/002) against throwaway repositories
python3 tests/test_history.py

# Run via CTest
ctest --test-dir build --output-on-failure
```

---

## Contributing & Adding New Rules

Every rule in UVMCheck inherits from `uvmcheck::rules::Rule`:
1. Create `src/rules/<category>/YourRule.cpp`.
2. Implement either `slang::ast::ASTVisitor` (typed AST) or `slang::syntax::SyntaxVisitor` (CST/tokens).
3. Register the rule factory in `src/rules/BuiltinRules.cpp`.
4. Add unit tests in `tests/rules/<category>/<RULE_ID>/bad.sv` and `good.sv`.
5. Run `python3 tests/regression.py --rule <RULE_ID>` to verify.

See [CONTRIBUTING.md](CONTRIBUTING.md) for full development guidelines, memory-bounded compilation instructions, and pull request conventions.

---

## License

This project is licensed under the MIT License - see the [LICENSE](LICENSE) file for details.
