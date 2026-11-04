# Rust front end

The plan to give every DV tool one design model, with slang moving behind it
layer by layer. R0 (the bridge) is done; R1 (shared API and tools) is under way.

```
tools: dv-map, dv-filelist (later dv-lsp, dv-fmt, ...)    Python: uvmcheck.model
  ├─ sv-source    VCS/Xcelium file lists, native Rust
  └─ dv-model     safe, owned Rust snapshot of the design
       └─ sv-slang   raw FFI to libuvmcheck_model (C ABI)  <- Python uses the same ABI
            └─ uvmcheck::model  (C++ facade, include/uvmcheck/model/)
                 └─ slang
```

The model holds every class, parameterized ones included (one entry per
definition; `uvm_driver #(pkt)` points at `uvm_driver`).

`uvmcheck::model` is also what new C++ rules read (`UvmModel::facade()`), so
rules and Rust tools see the same data. SV-OOP-007 and UVM-SEQ-002 already run
on it.

## Build

```sh
cmake --preset model && cmake --build build-model   # libuvmcheck_model.so + model_capi_smoke
cd rust && cargo test && cargo build --release
```

The `model` preset is a separate build: it compiles slang position independent
and without mimalloc, which a shared library needs, while the default preset
keeps the linter CLI as it was. `UVMCHECK_BUILD_DIR` points the Rust build at
another directory.

## dv-map

```sh
rust/target/release/dv-map -- +incdir+tests/common tests/map/map_tb.sv
rust/target/release/dv-map --json -- -f my_files.f
rust/target/release/dv-map --bench 5 -- +incdir+$UVM/src $UVM/src/uvm_pkg.sv
```

`--bench` prints the per-stage cost (parse, elaborate, model build, copy into
Rust), which is the R0 gate: the Rust side must add under 10% to the C++ load.

## dv-filelist

```sh
rust/target/release/dv-filelist -f top.f                # summary + every problem as file:line
rust/target/release/dv-filelist --args -f top.f > flat.f  # normalized, flat list uvmcheck reads
rust/target/release/dv-filelist --json --dialect xcelium -f top.f
rust/target/release/dv-filelist --load -f top.f          # also compile it through dv-model
```

It follows `-f`/`-F` nesting (`-F` paths are relative to the list), expands
`$VAR`, `${VAR}` and `$(VAR)`, understands `+incdir+`, `-incdir`, `+define+`,
`-define`, `-D`, `+undefine+`, `-y`, `-v`, `+libext+`, `-libext`, `-top`,
`-timescale`, `-uvm`, `-uvmhome` and `-ntb_opts uvm-x.y`, and keeps simulator-only
options (`-l run.log`, `+acc`) as ignored. Exit status 1 means problems found.

## Python

```python
from uvmcheck.model import Design      # python/uvmcheck/model.py, standard library only
d = Design.load(["-f", "files.f"])
```

## Why a C ABI and not cxx

The plan named `cxx`. A plain C ABI does the same job for Rust with no code
generation, and also serves Python (`ctypes`), C tools and any future language,
which is the point of a central library. `cxx` can still be layered on later.

## sv-lexer (R2, first piece of the native front end)

A lossless SystemVerilog lexer in plain Rust, no Slang: every byte of the
input is in exactly one token (trivia included), so the tokens give the
file back byte for byte. Directives and macro punctuation are tokens; the
preprocessor that expands them comes next.

```
cargo run --release -p sv-lexer --example lexstat -- <dir-or-file>...
```

Measured 2026-11-03: uvm-core + this repo's tests (952 files, 3.2 MB) and
sv-tests (1,029 files) all round-trip; no Unknown tokens except one sv-tests
case that is meant to fail (a macro splitting a string). About 110 MB/s.

Punctuation follows Slang's token set (so `[*`, `[->` and `.*` are two
tokens, and based literals are an `IntegerBase` plus digits), which keeps
the preprocessor's output comparable token for token.

## sv-preproc (R2)

The SystemVerilog preprocessor on top of sv-lexer: `define with defaults,
`undef/`undefineall, `ifdef/`ifndef/`elsif/`else/`endif (including the
1800-2023 expression form), `include with include dirs, `__FILE__`/`__LINE__`,
stringify (`"), paste (``) and escaped quotes. It follows Slang's macro
semantics, including where whitespace lands inside stringified text.

```
cargo run --release -p sv-preproc --example ppdump -- +incdir+DIR... FILE
```

Parity with Slang is checked token for token by `tools/pp_parity.py`
against the `uvmcheck_pp_dump` reference tool. Measured 2026-11-03:
uvm-core's `uvm_pkg.sv` (278,335 tokens) is identical, and so are all 949
sv-tests files that are meant to pass (79 skipped as should-fail).

## sv-syntax (R2, lossless parser)

A syntax tree over sv-preproc's tokens. Every token is a leaf exactly once;
a token read from a file carries the source between it and the previous
token of that file (whitespace, comments, directives, macro uses) as
leading trivia, so `Tree::file_text` gives every file back byte for byte.
Nodes are built from parser events, so a finished node can be wrapped
later (for binary expressions).

Slice 1 (2026-11-03) parses design units, packages, classes, methods
(including extern and pure virtual prototypes and out-of-block bodies),
typedefs, constraints, covergroups, generate constructs and statements at
block level. Expressions, types, port lists and constraint bodies are
balanced token runs inside their nodes for now.

```
cargo run --release -p sv-syntax --example outline -- --check +incdir+DIR FILE
```

`tools/cst_parity.py` compares the outline (design units, classes,
functions, tasks, typedefs, constraints, covergroups, in order) with
Slang's parser via `uvmcheck_cst_outline`, and checks the round trip.
Measured on slice 1: uvm-core's `uvm_pkg.sv` (168 files, 5,402 entries)
identical with no errors in about 100 ms including preprocessing; all 849
sv-tests files Slang parses cleanly identical; all 772 of this repo's test
inputs Slang parses cleanly identical.

Slice 2 (2026-11-04) adds expressions (precedence climbing over Table
11-2, calls, selects, casts, concatenation, streaming, assignment
patterns, `new`, `with`, `inside`, patterns after `matches`), data types
and declarations (enums, structs, parameters, instantiations, gates,
non-ANSI ports) and statements built on them. `begin_keywords` is
honored by the preprocessor. `tools/cst_parity.py --exprs` compares every
top-level operator expression in prefix form, `(+ a (* b c))`, with
Slang's (`uvmcheck_cst_exprs`): uvm-core (8,849 expressions), all 849
clean sv-tests files and all 772 clean repo test inputs are identical,
with no parse errors; uvm_pkg.sv preprocesses and parses in about 120 ms.
Constraint bodies, covergroups, assertions, timing controls and
module/class/function headers are still kept as written (slice 3).

Slice 3 (2026-11-04) parses what slice 2 still kept as written: module,
class and function headers (parameter and port lists, extends,
implements), timing and event controls, constraint blocks (dist,
implication, if/foreach, unique, soft), covergroups (options,
coverpoints, bins, transitions, cross selections), properties, sequences
and concurrent assertions, clocking blocks, `let`, `default disable iff`,
DPI imports, modports and `bind`. The expression gate now covers all of
these: uvm-core (8,972 expressions), all 849 clean sv-tests files and all
772 clean repo test inputs are identical to Slang with no parse errors.
Still kept as written: specify blocks, UDPs, config, checkers, attribute
contents, nettype and randsequence (slice 4).

Slice 4 (2026-11-04) parses the rest: specify blocks (specparams, path
delays with edges, polarity and conditions, system timing checks), UDPs
(ports, `initial`, tables), configs, checkers, attribute contents,
`nettype`, net `alias`, elaboration tasks (`$error` and friends as items)
and randsequence (productions, weights, `rand join`, `if`/`repeat`/`case`
items, code blocks). Nothing is left out of the gates any more: Slang's
side of `tools/cst_exprs.cpp` skips nothing, and uvm-core, the 849 clean
sv-tests files and the 772 clean repo inputs still match.

It also reports what Slang's parser rejects. The preprocessor checks
`` `line ``, `` `pragma ``, `` `timescale `` values, `` `unconnected_drive ``,
directives that are not allowed inside a module, interface, program or
package, and macro names that are directives. After parsing, `checks.rs`
reports end labels that do not match, tasks with a return type, crosses
of one item and a `super.new` after other statements. On sv-tests every
file Slang's parser rejects (128, including the should-fail tests) is
rejected and no other file is; the same holds for the repo's inputs. The
outline example prints preprocessor diagnostics with `--errors` and the
tree with `--dump`.

## sv-sema (R3, native class model)

Builds `dv_model::Design`, the same model the Slang-backed facade gives,
from sv-syntax trees with no Slang: `sv_sema::load(args)` takes the usual
compile arguments (`+incdir+`, `+define+`, `-I`, `-D`, files). Slice 1
(2026-11-04) covers classes in packages and compilation units, nested
classes, typedefs, imports and inheritance through them; parameterized
classes (each definition once, with members read from one representative
specialization chosen as Slang does: the one that prints shortest);
properties, methods (extern bodies, pure, static, virtual by declaration
or override, UVM phases), arguments, locals and constraints; UVM roles from
the base chain; and the built-in methods every class gets (`randomize` and
friends; interface classes get all but `rand_mode` and `constraint_mode`).
Locations of macro-made members point where Slang points: into the
`define body (sv-preproc now records each token's written origin).

`dv-model` builds without Slang (`default-features = false`), which is how
sv-sema uses it; `Design::dump` is the canonical text form both sides print.

```
cargo run --release -p sv-sema --example model_dump -- [--slang] [--types] +incdir+DIR FILE
tools/model_parity.py --uvm UVM_SRC        # uvm_pkg.sv, Slang vs native
tools/model_parity.py --repo UVM_SRC       # every test and example input
```

Measured 2026-11-04: on this repo's 783 test and example inputs (32,731
classes) the two models are identical. On uvm-core's `uvm_pkg.sv` all 353
class lines are identical and 83 of 12,324 member lines differ. Those are
all arguments or properties typed by a type parameter of a generic class,
where the representative specialization differs: Slang only creates
specializations it elaborates, while sv-sema sees every one named in the
source, so for example `uvm_callbacks` is represented by a `uvm_event`
specialization instead of `uvm_mem`. The native model loads uvm_pkg.sv in
about 0.6 s including preprocessing and parsing.

Slice 2 (2026-11-04) adds method bodies (`body.rs`): calls (callee scope,
virtual and built-in flags, receiver as Slang prints it, `super.`, first
argument and whether it is `null`), assignments (sides, root and root
kind, non-blocking, output and inout arguments as writes), timing controls
and `super.new` with its statement index. The walker follows Slang's
binding rules: method resolution through the base chain and type
parameters, built-in methods of arrays, strings, enums, events,
covergroups and `std` classes, default arguments walked in the callee's
context, and constructor calls only for classes that declare `new`. A body
Slang rejects (an undeclared type or subroutine, `super` of an undeclared
base, timing in a function, `new` of a virtual class, an assignment used as
an `if` condition or chained) keeps only its timing controls, as in Slang.

```
tools/model_parity.py --bodies --uvm UVM_SRC
tools/model_parity.py --bodies --repo UVM_SRC
```

Measured 2026-11-04 with `--bodies`: 776 of the 783 repo inputs are
identical, and the class gate stays at 783. The other 7 are test inputs
Slang rejects for type errors sv-sema does not check (a downcast without
`$cast`, a reversed part select, a wrong array literal size, a blocking
write to a clocking output, an undeclared identifier, a redefined class),
plus one argument location inside a stringified macro argument. On
uvm_pkg.sv 22 call and 3 assignment lines differ: representative
specializations again, arguments of nested macros (Slang places them in
the inner `define), and receivers that start with a macro use. The
representative specialization Slang picks varies from run to run, so the
number of differing argument lines moves between about 20 and 50.

Type strings (`--types`) are next: Slang's canonical form spells out enum
values and parameter-dependent widths, so it needs constant evaluation.
Classes declared inside modules are not compared yet.
