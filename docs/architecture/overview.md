# Architecture overview

UVMCheck has two engines, one rule set and one test suite. This page is the
map of how they fit together; [rust.md](rust.md) goes deeper into the Rust
workspace.

## The two engines

```
                    testdata/rules/<area>/<ID>/{bad,good}.sv   data/uvm-api/*.json
                                 │   (shared fixtures)              │ (shared packs)
              ┌──────────────────┴───────────────────┐              │
              ▼                                      ▼              │
  cpp/  uvmcheck (C++20)                 crates/  dv-lint (Rust) ◄──┤
        Slang front end                          native front end   │
        rules in cpp/src/rules/                  sv-source → sv-lexer → sv-preproc
        reporting, fixes, LSP,                   → sv-syntax → sv-sema
        baselines, waivers, Python rules         rules in crates/dv-lint
              │                                      ▲
              │ libuvmcheck_model (C ABI)            │ --slang: Slang as the reference
              └──► crates/sv-slang → crates/dv-model ┘
                   python/uvmcheck/model.py (same ABI)
```

- **uvmcheck** (`cpp/`) is the Slang-based CLI. It owns reporting (text, JSON,
  SARIF, HTML), fixes, baselines, waivers, project config and the Python rule
  SDK, and serves lint diagnostics over LSP (`uvmcheck --lsp`).
- **dv-lint** (`crates/dv-lint`) runs every uvmcheck rule on the native Rust
  front end, in parallel. With `--slang` it builds the design through Slang
  instead, so the two front ends can be compared.
- **libuvmcheck_model** (built by the `model` CMake preset) puts Slang's view
  of a design behind a C ABI. `sv-slang` binds it, `dv-model` wraps it in safe
  Rust, and `python/uvmcheck/model.py` reads the same ABI from Python.
- The other Rust tools: `dv-lsp` (code browsing and completion for the
  editor), `dv-build` (the native builder: parses, elaborates and reports errors as a
  compiler would), `dv-map`
  (the component tree of a UVM testbench) and `dv-filelist` (checks and
  normalizes VCS/Xcelium file lists).

## What is shared

| Folder | Read by | Purpose |
|---|---|---|
| `testdata/rules/` | ctest (`cpp/tests/regression.py`), `cargo test` (`dv-lint/tests/rules.rs`), the rule catalog, `--explain` | one `bad.sv` / `good.sv` pair per rule |
| `testdata/common/` | both engines' tests | `uvm_mock.svh`, `test_fixtures.svh`, real-UVM shim |
| `testdata/conformance/` | `cpp/tests/test_conformance.py`, `tools/quality/ast_coverage.py` | IEEE 1800 clause probes |
| `testdata/map/`, `testdata/model/` | `dv-map`, the C ABI, Python and Rust model tests | small designs with known facts |
| `data/uvm-api/` | CMake (embedded by `tools/codegen/embed_uvm_api_packs.py`) and `dv-lint` (`include_str!`) | UVM API per release |

## How the engines are checked against each other

- `tools/oracles/*.cpp` are small Slang programs that print what Slang sees
  (preprocessor output, syntax outline, symbols, expression types). They are
  built by the top-level CMakeLists.txt.
- `tools/gates/` compare the Rust side with them: `pp_parity.py`,
  `cst_parity.py`, `sym_gate.py`, `expr_gate.py`, `model_parity.py`,
  `ieee_gate.py` (sv-tests), `slang_unit_gate.py`, `spec_gate.py`, and
  `lint_parity.py` (dv-lint findings against uvmcheck's).
- The real-code corpus (UVM releases, riscv-dv, ibex, core-v-verif, cva6,
  common_cells) and its gate live outside git, under the ignored
  `references/` folder.

## Build trees

| Command | Output |
|---|---|
| `cmake --preset default && cmake --build build` | `build/uvmcheck`, the oracles |
| `cmake --preset model && cmake --build build-model` | the same plus `build-model/libuvmcheck_model.so` (position independent, no mimalloc) |
| `cargo build --release` (repository root) | `target/release/dv-lint`, `dv-lsp`, `dv-build`, `dv-map`, `dv-filelist` |

The Rust build finds the model library in `build-model/`, or wherever
`UVMCHECK_BUILD_DIR` points.
