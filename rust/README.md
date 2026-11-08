# Rust front end

A native SystemVerilog/UVM front end in Rust and the DV tools built on it:
lexer, preprocessor, lossless parser, semantic layer (class model,
elaboration, compiler checks), and the linter (`dv-lint`, every uvmcheck
rule), builder (`dv-build`), language server (`dv-lsp`), `dv-map` and
`dv-filelist`. Slang stays reachable behind a C ABI as the reference the
parity gates compare against.

**[ARCHITECTURE.md](ARCHITECTURE.md)** is the map: what each crate does, how
a compile flows through them, how the linter runs rules in parallel, and
which gate checks which layer. The sections below are the build, the tools'
usage and the record of each stage as it landed (R0 bridge, R2 parser, R3
model and rules, N0-N6 builder phases), with what was measured then.

### The slang bridge (R0)

```
tools                                                      Python: uvmcheck.model
  └─ dv-model     safe, owned Rust snapshot of the design
       └─ sv-slang   raw FFI to libuvmcheck_model (C ABI)  <- Python uses the same ABI
            └─ uvmcheck::model  (C++ facade, cpp/include/uvmcheck/model/)
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
cargo doc --workspace --no-deps --open               # API docs of every crate
```

The `model` preset is a separate build: it compiles slang position independent
and without mimalloc, which a shared library needs, while the default preset
keeps the linter CLI as it was. `UVMCHECK_BUILD_DIR` points the Rust build at
another directory.

## dv-map

```sh
rust/target/release/dv-map -- +incdir+testdata/common testdata/map/map_tb.sv
rust/target/release/dv-map --json -- -f my_files.f
rust/target/release/dv-map --bench 5 -- +incdir+$UVM/src $UVM/src/uvm_pkg.sv
```

`--bench` prints the per-stage cost (parse, elaborate, model build, copy into
Rust), which is the R0 gate: the Rust side must add under 10% to the C++ load.

## dv-lsp

A language server for code browsing on the native front end (sv-syntax, no
Slang): document outline, workspace symbols, go to definition / declaration /
type definition, references, highlights, hover (declaration plus its doc
comment) and completion (scope names with inherited class members, `obj.` and
`Class::`/`pkg::` members, `` ` `` macros, `$` system tasks, `type_id::`).
Lint diagnostics and quick fixes stay with `uvmcheck --lsp`; the VS Code
extension (editors/vscode) runs both.

```sh
cargo build --release -p dv-lsp
rust/target/release/dv-lsp --index $UVM_HOME/src   # 170 files in ~50 ms
```

Every `.sv/.svh/.v/.vh` file under the workspace is indexed on its own
(includes are not followed, so positions stay in the file that wrote them),
in parallel, and re-indexed on each edit. `initializationOptions` take
`libraryPaths` (extra trees such as UVM; `$UVM_HOME/src` is added by itself)
and `exclude` (directory names). Name resolution is by scope (locals, class
members through `extends`, file level, imported packages), then by name across
the workspace; references are textual matches of the name.

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
uvm_pkg.sv 16 call and 3 assignment lines differ (22 before slice 3): representative
specializations again, arguments of nested macros (Slang places them in
the inner `define), and receivers that start with a macro use. The
representative specialization Slang picks varies from run to run, so the
number of differing argument lines moves between about 20 and 50.

Slice 3 (2026-11-04) adds type strings (`types.rs`): property, argument,
local and return types as Slang's `Type::toString` prints them, and each
argument's canonical type. Builtins keep only the signing that differs
from the default (`int unsigned`, `bit signed`); packed and unpacked
bounds are evaluated (`logic[W-1:0]` prints `logic[7:0]`, `x[3]` prints
`$[0:2]`); typedefs and type parameters print with their scope path
(`uvm_pkg::uvm_queue::T`); classes print as `name#(args)` with value
arguments in Slang's number format; virtual interfaces as `ifc.mp` or
`ifc#(W=8)`; canonical types expand the alias chain, enums with every
value (`enum{A=32'sd0,...}e_t`). A small constant evaluator covers
literals, parameters (with specialization bindings), enum members,
operators, `$clog2` and `$bits`. Representative specializations are now
chosen by their full printed form, value arguments included, which also
narrowed the body and class gaps.

```
tools/model_parity.py --types --uvm UVM_SRC
tools/model_parity.py --types --repo UVM_SRC
```

Measured 2026-11-04 with `--types`: 762 of the 783 repo inputs are
identical. Of the other 21, 20 differ only in the index Slang gives an
anonymous struct type (`s$5`), a global counter in the order it happens to
create them, and one uses a class before declaring it, which Slang rejects
as `<error>`. On uvm_pkg.sv about 90 of 12,324 lines differ (it varies run
to run with Slang's representative choice), all in arguments typed through
nested generic classes, plus the anonymous type indices.

Classes declared inside modules need no work: Slang's model only walks
packages, `$unit` and nested classes, and so does sv-sema.

## dv-lint (R3 slice 4, rules without Slang)

The C++ rules that read only the model facade (`UvmModel::facade()`), ported
line for line with their messages verbatim. `dv-lint` runs them on a design
built natively by sv-sema, or through Slang with `--slang`, and prints findings
in uvmcheck's text form:

```
cargo build --release -p dv-lint
target/release/dv-lint [--slang] [--rule ID]... [--list] <compile options>
```

As in uvmcheck, a finding on code from a macro is reported at the line of the
outermost expansion with column 0, and nothing is reported in the test
`uvm_mock.svh`. For that, `Loc` carries the expansion site (`expansion` in
Rust, `exp_file`/`exp_line` in the C ABI, now version 6).

`tools/lint_parity.py` runs `uvmcheck --preset all --experimental --json`,
keeps the findings of the 27 rules, and compares them with dv-lint's as
multisets of file name, line, column, rule and message:

```
tools/lint_parity.py [--slang] --repo UVM_SRC
tools/lint_parity.py [--slang] --uvm UVM_SRC
```

Measured 2026-11-04 on the native model: all 783 repo compiles are identical
(465 findings; every one of the 27 rules fires at least once). On uvm_pkg.sv
150 of 152 findings match; the other 2 are SV-PRF-001 messages that print an
anonymous struct type with Slang's `s$N` index.

Slice 5 (2026-11-04) made the native load faster without changing any gate.
Most of the cost was in placing locations: each one counted newlines from the
start of its file. Line starts are now indexed once per file. Each tree node
also records its token span, so taking a node's tokens no longer walks the
node, and per-class lookups (base classes, base chains, virtual methods) are
remembered. On uvm_pkg.sv, best of 7 runs, the model build went from about
730 ms to about 165 ms; parsing takes about 125 ms. Through Slang, the same
load takes about 80 ms to parse, 160 ms to elaborate and 50 ms to build the
model. `dv-lint --time` prints these stage times.

Slice 6 (2026-11-04) moved four more rules onto the model: UVM-FAC-002,
UVM-FAC-003, UVM-PHS-001 and UVM-BAN-003 (31 in all). They needed three new
class facts, filled by both front ends: `is_interface`, the user macros used in
the class's text (`macros`, as Slang's top-level macro references: no uses
inside macro bodies or arguments, plus `` `undef `` of user macros), and the
class's `typedefs` by name. The C ABI is now version 7. On the native model
all 783 repo compiles are identical (906 findings) and uvm_pkg.sv keeps only
the two `s$N` messages; through Slang both are exact.

### Syntax rules (R3 slice 7)

About 150 of the C++ rules still on slang read only the syntax tree (a
`SyntaxVisitor`, no symbols or types). dv-lint now runs such rules on the
native sv-syntax trees: `dv_lint::syntax::Cst` gives a rule the tree's nodes
in source order, parents, direct tokens, slang-style `toString()` text and
locations numbered like the design's files (`sv_sema::Locator`, shared with
the model builder). `sv_sema::load_with_trees` hands the trees over, and
`dv_lint::lint_all` runs model and syntax rules together.

sv-syntax has fewer node kinds than slang, so each port states the slang
shape it stands for: a case statement's `default` is a direct token, an
assignment is a `BinaryExpr` whose operator is `=`, `<=` right under an
expression statement is a nonblocking assignment, and `deassign`/`release`
are not slang's ProceduralAssignStatement.

The first 13 are SV-COR-006/008/009, SV-DEP-001/002/003/005, SV-DED-009/010,
SV-DLY-002, SV-RCE-001 and SV-CON-005/007 (44 rules in all). With
`--slang`, only model rules run. `tools/lint_parity.py --dir DIR` checks each
`.sv` file under a directory on its own. Measured 2026-11-05: repo 783/783
(941 findings), uvm_pkg.sv only the two `s$N` messages, and all 1028
sv-tests files identical. On uvm_pkg.sv all 44 rules take about 23 ms.

Slice 8 (2026-11-05) added 17 more syntax rules (`syntax2.rs`): SV-NAM-002/003,
SV-TYP-010, SVA-LBL-001, SV-RND-009/014, SV-COV-003/007/008/009/014/015,
SV-CON-004/010, UVM-PHS-007, SV-BAN-001 and SV-DEP-006 (61 rules in all).
Slang shapes they stand for: a declaration with a direction is a
PortDeclaration and one inside a struct type a StructUnionMember (not a
DataDeclaration); a header port list whose first port is a bare name or
`.x(...)` is non-ANSI; `x.randomize() with {...}` is a call with a
`WithClause`. Measured: repo 783/783 (1004 findings), uvm_pkg.sv only the
two `s$N` messages, sv-tests 1027/1028. The one left, `sanity.sv`, is a
should-fail file where slang's error recovery keeps a `wire` declaration that
sv-syntax skips with the bad item.

## dv-build (builder phase N0)

The native builder: `dv-build` reads compile options and VCS/Xcelium file
lists (through sv-source), parses every source with the native front end and
reports problems as a compiler would (`file:line:col: error: message`, then
`errors N`; exit 1 on errors). `--dump` prints the declarations in source
order. Each builder phase (scopes for every design unit, types, constants,
elaboration, type checking) adds its checks here.

`tools/ieee_gate.py SV_TESTS` runs every sv-tests file through dv-build and
through uvmcheck (Slang) and reports per IEEE 1800 clause how many legal
tests each accepts and how many `should_fail` tests each rejects; `--gaps`
lists the ones only Slang rejects. Set `UVM_SRC` so UVM tests compile
`uvm_pkg.sv` first, as the sv-tests runner does. Baseline 2026-11-05: legal
949/949 native (Slang 948/949; the one is a clause 22 test), should_fail
25/75 native against 73/75 Slang (4 more are runtime-only). The 48 gaps are
semantic checks, closed by phases N1 to N5.

### Phase N1: scopes and name checks

`sv-sema::scopes` builds a scope for every design unit: `$unit`, packages,
modules, interfaces, programs (nested ones too), classes and subroutines,
each with the symbols it declares (parameters, type parameters, ports and
the net or variable each names, nets, variables, typedefs, enum values,
subroutines, arguments, constraints, covergroups, modports, clocking blocks,
genvars, sequences, properties, lets, net types, specparams, instances,
`bind` instances in their target). Generate and statement blocks are
elaboration's (N4). `dv-build --symbols` prints it as sorted
`<scope path> <kind> <name>` lines, the form of the Slang reference
`build/uvmcheck_sym_dump` (tools/sym_dump.cpp, which default-instantiates
every definition). `tools/sym_gate.py SV_TESTS` and
`tools/sym_gate.py --repo UVM_SRC` compare the two: 949/949 legal sv-tests
and 783/783 repo files identical.

`sv-sema::names` checks what needs only the scopes: redefinitions in a
scope, forward typedefs never defined, typedefs of undeclared types,
`extern constraint` without a body, overriding `randomize`, `rand_mode` or
`constraint_mode`, `pure virtual` methods in a non-virtual class, `new` of a
virtual or interface class, and undefined randsequence productions. Each
check only reports what it can see for sure. The IEEE gate after N1: legal
949/949 (no false errors, also none on the repo's 783 files), should_fail
40/75 native; 33 gaps left, all needing types or expressions (N2, N5).

### Phase N2 slice 1: the type of every declaration

`sv_sema::symbol_types` gives each net, variable, parameter, argument and
typedef of the scopes its type as Slang prints it, using the class model's
type engine (types.rs), which now also has scopes for modules, interfaces
and programs (`Builder::designs`, header imports, `type` parameters in
bodies). Parameters declared without a type take their value's; nets of a
user net type take its data type. `dv-build --types` prints the dump with
` : <type>`; `build/uvmcheck_sym_dump --types` is the reference and
`tools/sym_gate.py ... --types` compares them: 943/949 legal sv-tests and
779/783 repo files identical. The rest need `type(expr)`, constant
functions or replications (N3), x/z enum values, and `weak_reference`.
Not compared: members of parameterized classes (Slang prints a type
parameter by name or value depending on which specialization it made
first; N4 checks specializations), the global `s$N`/`e$N` counters, and six
UVM typedefs whose printed arguments come from Slang's specialization cache.

### Phase N2 slice 2: type checks

`sv-sema::checks` resolves names through the scopes (and the blocks between)
and rejects: bit-selects of a `real` and `real` indexes, edges of a `real`,
packed struct member defaults, procedural assignments to nets (also through
clocking outputs), unparenthesized assignments inside expressions, `return`
with a value in a void function, `fork ... join`/`join_any` in a function,
`return` in a fork, enum x/z values on a two-state base, a name after an
x/z value with none, sized enum values of the wrong width, integers assigned
to enums and compound assignments to them, parameters that use a specparam,
`randc` in `solve ... before`, soft constraints and distributions, and
unimplemented inherited pure constraints. A name it cannot resolve for sure
(inherited, imported, hierarchical) is never an error. IEEE gate after N2:
legal 949/949 with no false errors (and none on the repo's 783 files),
should_fail 60/75 (Slang 73/75); 13 gaps left: interface-class rules (8.26),
randsequence variables, streams, slice widths, struct array patterns and
undefined parameters in typedef ranges.

### Phase N3: constants and parameters

`dv-build --values` prints the value of every parameter, localparam,
specparam and enum value the way slang prints its `ConstantValue`
(`uvmcheck_sym_dump --values` is the reference; `tools/sym_gate.py
--values` compares them). Besides expressions, `sv-sema::consteval` runs
constant functions (13.4.3) on a small interpreter (declarations,
assignments, `if`, blocks, `for`, `while`, `repeat`, `return`), and
evaluates concatenations, replications and elements of constant unpacked
arrays. x and z digits read as 0 only for a two-state target; `'0`/`'1`
fill the target; `$` and `'{...}` aggregates print as slang prints them.
Anything it cannot evaluate gives no value, never a wrong one. Value gate:
sv-tests 949/949, repo 783/783.

### Phase N4: elaboration

`dv-build --hier` prints the elaborated design the way
`uvmcheck_sym_dump --hier` prints Slang's: from each top module and program
down, every instance (instance arrays expanded), every generate block that
is taken (loops unrolled, `if` and `case` decided, unnamed blocks named
`genblk<n>`, a directly nested `if` kept in its construct) and every value
parameter with its value in that instance. `sv-sema::elab` handles named
and positional overrides, type parameters, defparams (before or after the
instance), `bind` to a module or to instances, package functions, casts,
`$bits` of type names and packed structs. A parameter that cannot be
evaluated is left out, and nothing that depends on it is guessed from its
declaration. Elaboration also rejects indexed part-selects with a constant
width that is not positive. Gate (`tools/sym_gate.py --hier`): sv-tests
949/949, repo 783/783; on PULP common_cells 1903 of 1926 lines match with
no wrong value (the rest are packed values wider than 128 bits). That run
also fixed the parser for pasted `/``* *``/` comments in macros, typed
ports and parameters with packed dimensions, attributes on port
connections and doubly parenthesized implications. IEEE gate after N4:
should_fail 61/75.

### Phase N5: binding and class rules

`sv-sema::bind` rejects identifiers that name nothing, as values or as
types. A name must be declared in an enclosing class (or a class it
extends, not one it implements: 8.26.3), design unit, package it imports,
or the compilation unit; out-of-block methods and constraints see their
class. It is lenient where it cannot be exact: a name declared anywhere in
an enclosing design unit counts, a scope with an unknown base class or
package is not checked, and names the hierarchy may resolve (called
subroutines, the first name of a dotted path, bind, defparam) are left
alone; implicit nets, `foreach` variables, pattern variables and `with`
iterators count as declared. `sv-sema::iclass` checks interface classes:
`implements` of a type parameter or of a class defined later, methods and
type names inherited with conflicting definitions (also through two
specializations of one class), and `C::x` of a parameterized class without
`#()` (8.25.1). `checks` adds streams wider than their target and
C-like flattened array patterns. IEEE gate after N5: should_fail 73/75,
the same as Slang, legal 948/949 (the one rejected file Slang rejects too),
and no false error on the repo or PULP common_cells.
