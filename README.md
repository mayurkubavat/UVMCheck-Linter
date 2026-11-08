# UVMCheck

Static analysis for SystemVerilog (IEEE 1800) and UVM (IEEE 1800.2)
verification code: UVM methodology, OOP design, concurrency and delta-cycle
races, SVA and coverage health, constrained random, code smells, duplication
and style. Findings come as text, JSON, SARIF 2.1.0 or a self-contained HTML
report, with baselines, waivers, safe autofixes, a language server and a
VS Code extension.

There are two engines that share one rule set and one test suite:

| Engine | Where | Front end | Binary |
|---|---|---|---|
| **uvmcheck** (C++20) | [`cpp/`](cpp/) | [Slang](https://github.com/MikePopoloski/slang) | `build/uvmcheck` |
| **dv-lint** and the DV tools (Rust) | [`crates/`](crates/) | native lexer, preprocessor, parser and elaborator (Slang only as an opt-in reference) | `target/release/dv-lint`, `dv-lsp`, `dv-build`, `dv-map`, `dv-filelist` |

Every rule has a `bad.sv` / `good.sv` pair under [`testdata/rules/`](testdata/rules/)
that both engines are tested against, and parity gates check that their
findings match on real code. The rule catalog is in
[docs/rules/README.md](docs/rules/README.md).

## Install

Build from source. The Rust tools need only a Rust toolchain (1.82+); the C++
engine needs a C++20 compiler (clang 15+, AppleClang 15+ or gcc 12+), CMake
3.28+, Ninja and Python 3.9+, and fetches Slang at configure time. `ccache`
is recommended.

```sh
git clone https://github.com/mayurkubavat/UVMCheck-Linter.git
cd UVMCheck-Linter

# dv-lint and the other Rust tools (native front end, no C++ build)
cargo build --release            # -> target/release/dv-lint, dv-lsp, dv-build, dv-map, dv-filelist

# uvmcheck, the C++ reference engine on Slang (optional: the parity
# gates, and the fallback for options dv-lint does not take)
cmake --preset reference
cmake --build build -j 4         # -> build/uvmcheck
```

[`tools/front/uvmcheck`](tools/front/uvmcheck) is a `uvmcheck` command that
runs dv-lint and falls back to the C++ binary for options dv-lint does not
take yet; put it (or a link to it) on your `PATH`. Release builds of
`uvmcheck` and the VS Code extension are attached to tagged
[releases](https://github.com/mayurkubavat/UVMCheck-Linter/releases).

## Quick start

```sh
# Lint a testbench with the methodology profile
uvmcheck tb/*.sv -I tb --preset methodology

# The same with dv-lint directly, as JSON or SARIF for CI
target/release/dv-lint +incdir+tb tb/top.sv --preset methodology
target/release/dv-lint -f tb.f --json findings.json --sarif findings.sarif

# Every rule, including experimental ones
target/release/dv-lint -f tb.f --preset all --experimental
```

Both engines take simulator-style compile options (`-f`, `+incdir+`,
`+define+`, `-top`), read `.uvmcheck/` project config, and honor waivers and
baselines. The Rust tools reach Slang only for the parity gates, behind the
`slang` Cargo feature (off by default); see
[third_party/slang](third_party/slang/README.md).

## Rules

409 rules in 20 categories. The largest groups:

| Area | What it catches |
|---|---|
| UVM (94) | factory and `config_db` misuse, phasing and objections, sequences, RAL maps and adapters, deprecated API |
| Style, naming, maintainability (80) | layout, naming conventions, long routines, deep nesting, god classes, magic numbers, duplication |
| Correctness, concurrency (60) | width mismatches, delta-cycle races, fork/join and clocking pitfalls, unchecked failures |
| Randomization, coverage, SVA (56) | unchecked `randomize()`, constraint conflicts, coverage that cannot close, vacuous assertions |
| OOP, architecture (33) | missing `virtual`, unsafe casts, layer violations, coupling and reach-through |

Profiles (`--preset`) pick a set for a purpose, such as `methodology`. The
generated [rule catalog](docs/rules/README.md) lists every rule with its
severity, profiles and `bad.sv`/`good.sv` examples, and the
[UVM Methodology Guide](docs/methodology/README.md) explains the reasoning
behind the design rules.

The full uvmcheck guide (profiles, fixes, reports, Python rules, baselines,
waivers, CI) is [docs/guide/uvmcheck.md](docs/guide/uvmcheck.md). The Rust
tools are described in [docs/architecture/rust.md](docs/architecture/rust.md).

## Repository layout

```
cpp/               uvmcheck: src/, include/uvmcheck/, tests/ (drivers of the binary)
crates/            Rust workspace members (Cargo.toml is at the root)
python/            Python SDK: rules, waivers and project config in Python (+ examples/, tests/)
editors/vscode/    VS Code / Cursor extension
testdata/          fixtures both engines read: rules/, common/, conformance/, map/, model/
data/uvm-api/      UVM API packs, compiled into both engines
tools/oracles/     Slang reference dumpers the gates compare against (built by CMake)
tools/gates/       parity and conformance gates
tools/codegen/     generators run by the build and for the docs
tools/quality/     precision, config sweep, AST coverage, Slang canary
third_party/slang/ the Slang pin, local patches (none today) and how to update it
docs/              guides, architecture, rule catalog, methodology guide
.uvmcheck/         this repository's own lint config
```

[docs/README.md](docs/README.md) is the index of all documentation, and
[docs/architecture/overview.md](docs/architecture/overview.md) explains how
the pieces fit together.

## Use it in CI

The repository is a GitHub composite action ([action.yml](action.yml)) and a
pre-commit hook ([.pre-commit-hooks.yaml](.pre-commit-hooks.yaml)); see
[CI, pre-commit and editor](docs/guide/uvmcheck.md#ci-pre-commit-and-editor).

## Tests

```sh
ctest --test-dir build                 # uvmcheck suites
ctest --test-dir build-model           # + C ABI, Python binding, cargo test --features slang
python3 cpp/tests/regression.py --rule UVM-CFG-002
cargo test                             # Rust, native only
tools/gates/rust_gate.sh               # cargo fmt --check + Clippy -D warnings
```

## Contributing

See [CONTRIBUTING.md](CONTRIBUTING.md) for how to add a rule and its tests.

## License

Apache License 2.0, see [LICENSE](LICENSE) and [NOTICE](NOTICE). Third-party components are listed in [THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md).
