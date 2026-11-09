# Architecture of the Rust workspace

This is the map of the Rust workspace (`Cargo.toml` at the repository root,
crates in `crates/`): what each crate does, how a SystemVerilog compile flows
through them, and where to start when changing something.
[rust-history.md](rust-history.md) has the build commands, tool usage and the history
of each stage with its measurements; `cargo doc --workspace --no-deps --open`
has the API of every crate.

## The pipeline

```
compile options / file lists (-f, +incdir+, +define+, -top, ...)
        │
        ▼
  sv-source ── FileList: sources, include dirs, defines, tops, problems
        │
        ▼
  sv-lexer ─── lossless tokens (trivia, directives and macro punctuation included)
        │
        ▼
  sv-preproc ─ `define / `ifdef / `include expanded; every token keeps where it came from
        │
        ▼
  sv-syntax ── lossless syntax tree per file (or one tree per unit)
        │
        ▼
  sv-sema ──┬─ class model ─────────► dv-model::Design ◄── sv-slang (slang, C ABI)
            ├─ elaboration (instances, generate, parameter values)
            ├─ semantic checks (names, types, constants, classes, SVA, UDPs)
            └─ type oracle, specializations, locations (for rules)
        │
        ▼
  tools: dv-lint (rules), dv-build (compiler-style errors), dv-lsp (editor),
         dv-map (UVM component tree), dv-filelist (file list doctor)
```

Everything from `sv-source` to `sv-sema` is native Rust with no C++ and no
slang. Slang is still reachable through `sv-slang` and `Design::load`, as
the reference the parity gates compare against and as `dv-lint --slang`,
but only behind the `slang` Cargo feature, which is off by default: a plain
`cargo build` or `cargo test` builds every tool natively and needs no C++
build, and `sv-slang` is not a default workspace member. The gates build
with `--features slang`; see `third_party/slang/README.md`.

## Crates

| Crate | Kind | Role |
| --- | --- | --- |
| `sv-source` | lib | Reads VCS/Xcelium file lists and compile options into a `FileList`, keeping the origin of every item and reporting problems instead of stopping. |
| `sv-lexer` | lib | Lossless lexer: every byte is in exactly one token, so tokens give the file back. Token set follows slang's so token streams compare one to one. |
| `sv-preproc` | lib | IEEE 1800-2023 clause 22 preprocessor. Output is the parser's token stream with no trivia or directives; each token records its file offset, the place its text was written, and whether a macro made it. |
| `sv-syntax` | lib | Event-based recovering parser. A `Tree` is a token array plus a node arena (`SyntaxKind` per node); trivia rides on tokens, so `Tree::file_text` rebuilds each file byte for byte. |
| `sv-sema` | lib | Everything slang did above syntax: scopes and name resolution, constant evaluation (arbitrary width, four-state), type strings, the class model, elaboration and the semantic checks a compiler makes. |
| `dv-model` | lib | The design model tools read: classes, UVM roles, properties, methods and body facts, all owned. Filled by `sv-sema` (native) or by slang through `sv-slang`. |
| `sv-slang` | lib | Raw FFI to `libuvmcheck_model` (`cpp/include/uvmcheck/model/CApi.h`). Only `dv-model` uses it, and only with the `slang` feature. |
| `dv-lint` | lib + bins | The linter: every uvmcheck rule ported to Rust, run in parallel over the model and the syntax trees. Its command line (`dv_lint::app`) is built under two names: `uvmcheck`, the product's command, and `dv-lint`, the name the gates call. |
| `dv-build` | bin | The native builder: reports file list, preprocessor, syntax and semantic errors like a compiler; dumps symbols, types, values and hierarchy for the gates. |
| `dv-lsp` | bin | Language server for code browsing (symbols, definition, references, hover, completion) on `sv-syntax`. Lint diagnostics come from `uvmcheck --lsp`. |
| `dv-map` | bin | Prints the static UVM component tree of a testbench (text or JSON). |
| `dv-filelist` | bin | Checks a file list, prints it normalized or as JSON. |

Dependencies point one way: `sv-lexer` ← `sv-preproc` ← `sv-syntax` ←
`sv-sema` ← tools. `dv-model` sits beside them; `sv-sema` depends on it to
fill a `Design`, and `dv-lsp` needs only `sv-syntax`.

## The front end in more detail

**Positions.** A preprocessed token carries an `Origin` (file index and
byte offset). For a token a macro produced, `origin` is the macro use and
`written` is where its text was written (in the `define body or a macro
argument). `sv_sema::Locator` turns these into `dv_model::Loc` (1-based
line and column) with one file table per design, and records the
expansion site, which is where uvmcheck reports findings on macro code.

**Compilation units.** `sv_syntax::parse_files` gives one tree per file
with macros shared across files, as simulators compile a file list.
`parse_unit` (`--single-unit`) gives one tree for all files.

**sv-sema's builder.** A private `Builder` walks every tree once to
collect declarations into scopes (`scopes`), then resolves names on demand
with constant evaluation (`consteval`, `packed`, `bigint`). From it come:

- `build` / `load`: the class model, including type strings as slang
  prints them (`types`) and method body facts (`body`);
- `elaborate_full`: the instance tree from the top modules (`elab`);
- `SemaChecks`: one module per family of checks (`names`, `checks`,
  `bind`, `iclass`, `classes`, `order`, `calls`, `typed`, `sva`, `udp`),
  run on several threads and merged by `semantic_diags`;
- `oracle::TypeOracle`, `SpecModel`, `spec_counts`: what rules ask beyond
  the model (the declared type of a name, a class seen through each
  specialization, how many specializations slang would elaborate).

Each check's messages follow slang's wording where slang has the same
check, so `dv-build`'s output can be compared with slang's.

## The linter

`dv_lint::rules()` lists every rule with a `Check`:

- `Model`: reads only `dv_model::Design`, so it runs on either front end;
- `Syntax`: reads one `syntax::Cst` (a tree with parent links, per-kind
  node lists and a locator), run once per tree;
- `Mixed`: reads the model and every tree of the compile at once.

`lint_all` runs them on `DV_LINT_JOBS` threads (default: all cores). Each
thread takes the next rule, with the slowest rules first. Work many rules
share (expression types in `expr_types`, sv-sema's checks, elaboration,
specialization counts) lives in `shared`: built once per compile by the
first thread that asks, while the others wait. Findings are merged back in
the serial order and sorted by position and rule, then inline waivers are
applied, so output is the same for any thread count. `DV_LINT_PROFILE=1`
prints the cost of each rule.

Every rule is a port of a C++ rule under `cpp/src/rules/` with the same id,
name, severity and message text; `tools/gates/lint_parity.py` compares the two
finding for finding.

The source is laid out by rule family, as `cpp/src/rules/` is:

```
crates/dv-lint/src/
  lib.rs             Rule, Check, lint_all and the parallel runner
  rules/mod.rs       collects every family
  rules/<family>/    one file per rule, named after it, plus mod.rs that
                     registers the family (uvm/, methodology/, random/, ...)
  support/           helpers several rules share, one module per topic
                     (class_index, statements, slang_ast, ...)
  syntax.rs          Cst: a tree with parents, per-kind node lists, locations
  expr_types.rs      expression types as slang gives them (--exprs)
  shared.rs          work built once per compile and shared between threads
  session.rs         Session: the settings one run of the rules reads
  uvm_api.rs, waivers.rs
```

A rule file such as `rules/uvm/blocking_in_observer.rs` opens with the
rule's id, name and what it flags, and holds the helpers only that rule
uses.

The settings a run reads (rule options, the `style` and `naming` blocks,
`-top`, `--skip-libraries`) live in a `dv_lint::Session`. A run hands them
to the threads it starts, so two sessions with different settings can lint
at the same time in one process: an editor with two workspaces, a test
harness, a host language. The command line uses the process's own session,
which is what the free functions (`dv_lint::options::set`,
`dv_lint::lint_all`) work on. Still held by the process, and so next to
move: the record of which waivers matched, the inline-waiver count, and the
git-history switch.

## The language server

`dv-lsp` indexes every `.sv/.svh/.v/.vh` file of the workspace on its own
(`index`: declarations with scopes, types and doc comments, plus every
identifier use), keeps a name table over all files (`workspace`), and
answers requests over stdio (`server`). Includes are not followed, so
positions always stay in the file that wrote them. `json` and `text` are
its small JSON and position/URI helpers; it needs no crates outside the
workspace.

## How correctness is checked

Each layer has a gate against slang, run from `tools/gates/` (the Rust side
built with `--features slang` where a gate uses `--slang`; most need
`UVM_SRC` pointing at a UVM source tree):

| Gate | Compares |
| --- | --- |
| `pp_parity.py` | preprocessed tokens |
| `cst_parity.py` | parse outline and canonical expressions (`--exprs`) |
| `model_parity.py` | `Design::dump` of the native and slang models (`--bodies`, `--types`) |
| `sym_gate.py` | scopes, types, values and hierarchy from `dv-build` |
| `ieee_gate.py` | errors on every sv-tests file (legal and should-fail) |
| `slang_unit_gate.py` | semantic errors on slang's own unit test cases |
| `expr_gate.py` | expression types from `dv-lint --exprs` |
| `spec_gate.py` | specialization counts from `dv-lint --specs` |
| `lint_parity.py` | dv-lint findings against uvmcheck's |

Plus `cargo test` (native) and `cargo test --workspace --features slang`
(also the Slang bridge and the tests that load through Slang), and
`rust_gate.sh`: `cargo fmt --check` (`rustfmt.toml`) and Clippy with
`-D warnings` on all targets, natively and with `--features slang` when
libuvmcheck_model is built. Clippy takes the MSRV from `rust-version` in
the root `Cargo.toml`. A change to the front end should keep
every gate where it was; a change to a rule should keep `lint_parity.py`
identical.

## Where to start

- A new or changed lint rule: open `rules/<family>/<rule_name>.rs`, or add
  a file there and register it in that family's `mod.rs` with a `Check` of
  the narrowest kind that works. Reuse
  `syntax::Cst` helpers and the caches in `shared` rather than rebuilding
  scopes or types in the rule.
- A parse problem: `sv-syntax` (`grammar`, `expr`, `blocks`, `assertions`,
  `misc`); check with `cst_parity.py`.
- A wrong type, value or error: `sv-sema`, starting from the module whose
  check reports it; check with `sym_gate.py` and `ieee_gate.py`.
- A file list option: `sv-source`.
