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

## Quick start

Requirements: a C++20 compiler (clang 15+, AppleClang 15+ or gcc 12+), CMake
3.28+, Ninja, Python 3.9+, and a Rust toolchain (1.75+) for the Rust tools.
`ccache` is recommended.

```sh
# uvmcheck (C++)
cmake --preset default
cmake --build build -j 4
build/uvmcheck tb/*.sv -I tb --preset methodology

# The Rust tools: native front end, no C++ build needed
cargo build --release
target/release/dv-lint +incdir+tb tb/top.sv
```

The Rust tools reach Slang only for the parity gates, behind the `slang`
Cargo feature (off by default); see [third_party/slang](third_party/slang/README.md).

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
```

## Contributing

See [CONTRIBUTING.md](CONTRIBUTING.md) for how to add a rule and its tests.

## License

MIT, see [LICENSE](LICENSE).
