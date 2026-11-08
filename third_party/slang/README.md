# Slang (third-party)

[Slang](https://github.com/MikePopoloski/slang) is the upstream SystemVerilog
compiler front end that `uvmcheck` (`cpp/`), `libuvmcheck_model` and the
reference dumpers in `tools/oracles/` are built on. It is not our code, so it
is kept out of this repository:

- **Pinned, fetched, never edited in place.** [`slang.cmake`](slang.cmake)
  holds the pin (`UVMCHECK_SLANG_TAG`) and fetches that commit at configure
  time into `<build>/_deps/slang-src`. Nothing under `cpp/` copies or forks
  Slang sources.
- **Workarounds live in our code, not in Slang.** Differences from simulators
  and Slang bugs are handled by the overlay in `cpp/src/compat/`
  ([docs/reference/compat.md](../../docs/reference/compat.md)): named
  overrides that use only Slang's public API, can each be turned off with
  `--override -<id>`, and name the upstream issue and the Slang major version
  that fixes it, so the overlay test fails once a workaround has outlived its
  bug.
- **Patches are a last resort.** Only when a fix cannot be made through the
  public API (a crash inside Slang, a missing hook) does a patch go into
  [`patches/`](patches/). It is applied automatically after the download.
  Today there are none.

## The `slang` Cargo feature: oracle and parity only

The Rust tools have their own front end (`sv-lexer`, `sv-preproc`,
`sv-syntax`, `sv-sema`) and do not need Slang. Slang is reachable from Rust
only through the C ABI bridge (`crates/sv-slang`, used by `crates/dv-model`'s
`Design::load`), and only behind a Cargo feature named `slang`:

- **Off by default.** `cargo build` and `cargo test` at the repository root
  build every tool (`dv-lint`, `dv-lsp`, `dv-build`, `dv-map`, `dv-filelist`)
  natively, with no C++ build and no libuvmcheck_model. `sv-slang` is not a
  default workspace member.
- **Never the default front end.** With the feature on, a tool still loads
  natively unless asked: `dv-lint --slang`, `dv-map --slang`,
  `dv-filelist --load --slang`, and `model_dump --slang` (sv-sema example).
  Built without it, `--slang` exits with an error that names the feature.
- **Who turns it on:** the parity gates (`tools/gates/lint_parity.py --slang`,
  `model_parity.py`) and ctest's `rust_model_test`
  (`cargo test --workspace --features slang`). Build them with
  `cargo build --release --features slang` after `cmake --preset model &&
  cmake --build build-model`.

Removing Slang from the Rust side later means deleting `crates/sv-slang`, the
`slang` features and the `#[cfg(feature = "slang")]` items; nothing in the
default build changes.

How deep the coupling goes: the C++ rules are written against Slang's AST and
syntax tree (`slang/ast/*`, `slang/syntax/*`), so an API change in Slang can
reach `cpp/src/rules/`. That is the price of using Slang's elaborated model;
the canary below tells you before a bump what breaks.

## Updating Slang

1. Run the canary against the candidate (master by default). It builds a
   separate tree, applies our patches, and runs the suites that measure Slang:

   ```sh
   tools/quality/slang_canary.py --work ../slang-canary [--ref <tag-or-commit>] \
       [--uvm-home ../uvm-core] --report canary.md
   ```

2. Read `canary.md`:
   - build failed: Slang's API changed; fix the C++ code that uses it;
   - a patch is "already upstream": delete it from `patches/`;
   - a patch "does not apply": rebase it on the new commit (below) or drop it;
   - `slang_overlay_test` fails naming an override: Slang fixed that bug;
     delete the override (or retire the crash pattern);
   - rule suites changed: decide whether each change is a Slang fix or a
     regression; report regressions upstream.
3. Move the pin: set `UVMCHECK_SLANG_TAG` in `slang.cmake` (a release tag when
   one has what we need, else a master commit, with its date and why).
4. Rebuild from clean (`rm -rf build build-model`, or at least
   `<build>/_deps/slang-*`), then refresh what depends on Slang's output and
   review the diff:

   ```sh
   cmake --preset default && cmake --build build && ctest --test-dir build
   python3 cpp/tests/regression.py --mode all-rules --update-baseline \
       --baseline cpp/tests/baselines/baseline_stub.json
   cmake --preset model && cmake --build build-model && ctest --test-dir build-model
   ```

   Rerun the parity gates in `tools/gates/` and the corpus gate, since they
   compare the native front end with Slang.
5. Commit the pin, the patch changes, the refreshed baselines and the canary
   report in one commit, so a bump can be reverted as a unit.

## Adding a patch

1. Check that an override in `cpp/src/compat/` cannot do it.
2. Open (or find) the upstream issue or pull request first.
3. Make the change in a Slang checkout at the pinned commit and export it:

   ```sh
   git -C <slang> diff > third_party/slang/patches/NNNN-<issue>-<short-name>.patch
   ```

   `NNNN` is the order they apply in; `<issue>` is the upstream number
   (for example `0001-2066-crash-in-randomize-with.patch`). Start the file with
   a few lines saying what it fixes and the upstream link; `git apply` ignores
   text before the first `diff --git`.
4. Reconfigure from a clean `<build>/_deps/slang-*` so the patch step runs.
   A patch that no longer applies stops the configure with its name.

Delete a patch as soon as the pinned commit contains the fix; the canary says
when ("already upstream").

`FETCHCONTENT_SOURCE_DIR_SLANG=<checkout>` builds against a local Slang
checkout (as the canary does). CMake skips its patch step then, and warns if
there are patches: apply them to that checkout yourself.
