# Measured vendor tables

Simulators accept SystemVerilog that IEEE 1800 forbids, and real code relies
on it. slang (and so uvmcheck) is strict by default and offers one option per
relaxation; `--compat vcs` turns on a fixed bundle of them. These tables say
what each tool actually accepts, measured, not copied from documentation.

Each probe in `testdata/vendor/` is legal SystemVerilog except for one
construct, and names the slang option that accepts it. `tools/quality/vendor_table.py`
compiles every probe with one tool and records the result in
`data/vendor/<tool>.json` with the tool's version and the date:

```
tools/quality/vendor_table.py measure verilator      # or iverilog, vcs, xcelium, questa, uvmcheck, dv-build
tools/quality/vendor_table.py table                  # this table, from data/vendor/
uvmcheck $(tools/quality/vendor_table.py slang-flags data/vendor/vcs.json) -f files.f
```

`slang-flags` prints the slang options for the relaxations a measured tool
accepts, so uvmcheck accepts the same code that tool does.

## Results

"no" can also mean the tool fails on something else in the probe (Verilator
5.020 has no covergroups); the JSON keeps each rejection's message.

| Probe | slang option | dv-build | iverilog | uvmcheck | verilator |
|---|---|---|---|---|---|
| `array-concat-pattern` | `--allow-array-concat-assign-pattern` | yes | no | no | yes |
| `bare-value-param` | `--allow-bare-value-param-assigment` | yes | no | no | yes |
| `cross-auto-bin-max` | `--allow-cross-auto-bin-max` | no | no | no | no |
| `declarations-after-statements` | `none` | yes | no | no | no |
| `genblk-reference` | `--allow-genblk-reference` | no | yes | no | yes |
| `hierarchical-const` | `--allow-hierarchical-const` | yes | no | no | no |
| `merging-ansi-ports` | `--allow-merging-ansi-ports` | no | no | no | no |
| `relax-enum` | `--relax-enum-conversions` | yes | no | no | no |
| `relax-string` | `--relax-string-conversions` | yes | no | no | no |
| `self-determined-stream` | `--allow-self-determined-stream-concat` | yes | no | no | yes |
| `toplevel-iface-ports` | `--allow-toplevel-iface-ports` | no | no | no | yes |
| `use-before-declare` | `--allow-use-before-declare` | no | yes | no | yes |

- dv-build: dv-build at 6c930fa (measured 2026-11-08)
- iverilog: Icarus Verilog version 12.0 (stable) () (measured 2026-11-08)
- uvmcheck: uvmcheck at 6c930fa (measured 2026-11-08)
- verilator: Verilator 5.020 2024-01-01 rev (Debian 5.020-1) (measured 2026-11-08)

## Not measured yet

VCS, Xcelium and Questa need their licences, so they are measured on a
machine that has them: `measure vcs` runs `vcs -full64 -sverilog`,
`measure xcelium` runs `xrun -sv -elaborate`, `measure questa` runs
`qrun -sv -optimize`, each with `probe` as the top. Commit the JSON it
writes; the table above picks it up.

## What the native builder still lets through

dv-build accepts seven probes that IEEE 1800 rejects (hierarchical names in
constants, implicit enum and string conversions, declarations after
statements, bare `#W` parameter values, self-determined streaming and
assignment patterns in array concatenations). Each is a semantic check
sv-sema does not make yet.
