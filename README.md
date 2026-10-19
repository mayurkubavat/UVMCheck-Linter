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
- **Modular Unit Testsuite**: 228 per-rule test directories with paired `bad.sv` and `good.sv` test cases running in ~3 seconds with 100% pass rate.

---

## Rule Domain Categories

| Category | Rules | Description |
| :--- | :---: | :--- |
| **UVM Methodology** | 44 | Factory registration, phasing, objection CFG balancing, cross-component config_db, TLM topology |
| **Concurrency** | 12 | Delta-cycle oscillations, `always_comb` triggers, fork-join variable capture, race conditions |
| **SVA (Assertions)** | 14 | Vacuous antecedents, combinatorial assert glitches, disable-iff resets, multi-clock domains |
| **Randomization** | 18 | Unchecked `randomize()`, non-pure functions in constraints, cyclic `solve-before`, randc exhaustion |
| **Coverage** | 16 | Cross-bin explosion, illegal bin overlaps, 0-weight coverpoints, unreferenced covergroups |
| **OOP & Architecture** | 22 | Non-virtual overrides, object slicing, downcast safety, virtual call in constructor |
| **Performance** | 18 | Deep-copying dynamic arrays by value, string concatenation in loops, unbounded arrays in constraints |
| **Correctness & Safety** | 30 | Off-by-one queue indexing, self-comparisons, non-blocking assignments in functions, casex hazards |
| **Types & DPI** | 15 | Mixed-sign comparisons, redundant `$cast`, non-void task return types, DPI width mismatches |
| **Deprecated & Smells** | 18 | OVM legacy APIs, banned `uvm_field_*` macros, `uvm_do` hiding, god-methods, magic delays |
| **Style & Formatting** | 21 | Mandatory end labels, line length limits, capitalization, naming conventions |

> For comprehensive rule specifications, rationales, and bad/good examples across all 228 rules, explore the [Rule Catalog](references/rules/catalog/uvmcheck_rule_catalog.md).

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
```

### Single-Rule Targeting
Run or debug a specific rule against a source file in milliseconds:
```bash
./build/uvmcheck tb/my_seq.sv --rule UVM-BAN-001
./build/uvmcheck tb/my_scb.sv --rule ScoreboardLeftoverQueue
```

### Automated Fix-It Rewrites
Automatically refactor compliant syntax without manual editing:
```bash
./build/uvmcheck tb/legacy_code.sv --fix
```

### Reporting (SARIF & HTML)
```bash
# Generate SARIF 2.1.0 for CI/CD code scanning
./build/uvmcheck tb/*.sv --sarif results.sarif

# Generate interactive HTML dashboard
./build/uvmcheck tb/*.sv --html report.html
```

### Custom Python Rules Plugin SDK
Verification teams can author custom corporate or project verification checks in pure Python without recompiling C++:
```bash
# Execute custom Python rules from a file or directory
./build/uvmcheck tb/*.sv --python-rules examples/python_rules/custom_naming_rules.py

# Python rule violations seamlessly participate in SARIF, HTML, and baselines
./build/uvmcheck tb/*.sv --python-rules my_rules/ --sarif results.sarif
```
See [Python Rule SDK Documentation](docs/python_rule_sdk.md) for full API reference, context query helpers, and tutorials.

---

## Baseline & Waiver Management

### Adopting on Existing Legacy Codebases
Establish a zero-new-defects baseline without fixing thousands of pre-existing legacy violations:

```bash
# 1. Snapshot all current legacy violations into baseline JSON
./build/uvmcheck tb/*.sv --update-baseline uvmcheck_baseline.json

# 2. Enforce zero regressions on new PRs (only new violations fail CI)
./build/uvmcheck tb/*.sv --baseline uvmcheck_baseline.json
```

### Waivers Configuration
Create a `uvmcheck_waivers.json` file to document approved exceptions:
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
      "reason": "Objection in loop verified for stress bench iteration"
    }
  ]
}
```
Run with waivers applied:
```bash
./build/uvmcheck tb/*.sv --waivers uvmcheck_waivers.json
```

---

## Regression & Unit Testsuite

UVMCheck features a modular unit test suite with 456 test cases covering all 228 rules:

```bash
# Run entire testsuite via Python runner (~2.8 seconds)
python3 tests/regression.py

# Run only UVM methodology tests (~0.5 seconds)
python3 tests/regression.py --category uvm

# Run single-rule test (~7 ms)
python3 tests/regression.py --rule UVM-CFG-002

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
