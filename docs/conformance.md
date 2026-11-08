# IEEE 1800 and 1800.2 conformance

uvmcheck aims to lint any code that is legal IEEE 1800-2023 SystemVerilog and
IEEE 1800.2 UVM. Slang does the parsing and elaboration; the pieces below keep
that claim measured, and make the parts slang cannot do explicit add-ons.

## LRM probes (`lrm_conformance_test`)

`testdata/conformance/lrm/` holds one probe file per clause group of IEEE
1800-2023 (clauses 5 to 35) plus one for the constructs 2023 added.
`testdata/conformance/clauses.json` maps each clause to its probe. The test runs
every probe with all rules on and fails when:

- uvmcheck crashes or does not write a JSON report;
- a probe reports a compile error (SV-CMP-001), meaning legal code was rejected;
- a clause from 5 to 35 has no probe, or a probe is not in the manifest.

Clauses with no compile-time meaning are marked `out_of_scope` with the reason:
32 (SDF back-annotation happens in the simulator) and 34 (protected envelopes
cannot be decrypted; the `opaque-ip` override handles the code around them).

When slang or uvmcheck rejects legal code, add the construct to its clause's
probe first; the test then fails until it is fixed or an override covers it.

## AST coverage (`ast_coverage_floor_test`)

Compiling a construct is not the same as checking it. `scripts/ast_coverage.py`
counts the slang AST kinds (symbols, statements, expressions, timing controls,
assertion expressions, constraints) that some rule or analysis names, and lists
the ones nothing inspects:

```
python3 scripts/ast_coverage.py               # table of inspected / total, and the gaps
python3 scripts/ast_coverage.py --json cov.json
```

`testdata/conformance/ast_coverage_floor.json` records the current counts. The
test fails when a category drops below its floor. When new rules inspect more
kinds, raise the floor in the same change.

## Language version

The `lang-std` override (on by default) compiles as IEEE 1800-2023 unless
`--std` names another version. Slang's own default is 1800-2017.

## UVM versions

`uvm::ApiPack` detects the UVM release being compiled (from `uvm_pkg`'s
version parameters) and loads its API pack from `data/uvm-api/`, generated from
the `@uvm-ieee`, `@uvm-accellera` and `@uvm-contrib` tags in uvm-core:

```
python3 scripts/gen_uvm_api_pack.py ../uvm-core --out data/uvm-api
```

Packs are embedded in the binary at build time; `UVMCHECK_UVM_API_PACK=<file>`
replaces them for one run. Rules UVM-API-001 (Accellera-only API) and
UVM-API-002 (deprecated macro) read only the pack, so a new UVM release needs a
new pack, not new code.

## Keeping up with slang

`scripts/slang_canary.py` builds uvmcheck against slang master in a separate
folder, runs the conformance, overlay and rule suites, and writes a Markdown
report. When an override's slang bug is fixed, or a known crash stops
crashing, the overlay test fails and names it, so the workaround is deleted
when the pin moves.

```
python3 scripts/slang_canary.py --work ../canary --uvm-home ../uvm-core --report canary.md
```

`uvmcheck --list-addons` lists the overrides, profiles and UVM API packs in
the binary.
