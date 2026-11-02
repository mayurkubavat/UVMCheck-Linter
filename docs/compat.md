# Slang overlay

uvmcheck compiles SystemVerilog with [slang](https://github.com/MikePopoloski/slang).
Slang follows IEEE 1800 closely and elaborates the Accellera UVM library cleanly,
but real projects are written for a simulator: their file lists use simulator
options, they expect the simulator to supply UVM, and they call the simulator's
waveform tasks. The overlay is a set of small overrides that bridge those gaps
without forking or patching slang. Every override is off until you turn it on,
except `anchor-to-user-code`, which fixes errors being hidden and is on unless
`--override -anchor-to-user-code` (or `"-anchor-to-user-code"` in `overrides`) turns it off.

```
uvmcheck --list-overrides
```

## Turning it on

A profile turns on the overrides a simulator's code usually needs:

```
uvmcheck -f sim/files.f --compat-profile questa
```

or in `.uvmcheck/config.py`:

```python
class Project(ProjectConfig):
    compat = {
        "profile": "questa",          # questa, xcelium, vcs or none
        "overrides": ["-auto-uvm"],   # "id" or "+id" turns one on, "-id" off
        "uvm_home": "$UVM_HOME",      # uvm-core checkout (or its src/) for auto-uvm
    }
```

`--override <id>` and `--override -<id>` on the command line apply after the
config, so one run can add or drop an override. The JSON report lists the
overrides that were active (`compatOverrides`), and they are part of the
result-cache key.

## Overrides

| Override | Profiles | What it does |
| --- | --- | --- |
| `filelist-dialect` | questa, xcelium, vcs | Accepts `-i <dir>` (include directory) and the UVM requests `-uvm`, `-uvmhome <dir>`, `-ntb_opts uvm`. Without it these options are an error that names this override. |
| `auto-uvm` | questa, xcelium, vcs | When the sources mention `uvm_pkg` or `uvm_macros.svh` (or a file list asks for UVM, including `-L uvm`) but no `uvm_pkg.sv` is compiled, compiles it from `compat.uvm_home`, `-uvmhome` or `$UVM_HOME`, and adds its include directory. |
| `uvm-library` | questa, xcelium, vcs | Treats the folder of every compiled `uvm_pkg.sv` as a library path, so warnings and findings inside UVM are not reported. |
| `vendor-systasks` | questa, xcelium, vcs | Knows `$fsdb*` (Verdi), `$vcdplus*` (VCS), `$shm_*` and `$record*` (Xcelium) and `$wlfdumpvars` (Questa). Their arguments may name instances, as with `$dumpvars`; a name that does not resolve is still an error. PORT-PRP-001 still reports unguarded calls. |
| `anchor-to-user-code` | on by default | An error slang raises while elaborating a library template for your class, such as a wrong constructor signature reported at `uvm_registry.svh:201`, is reported at your `` `uvm_*_utils `` line (or class) with the library line as a note; an error inside a library macro's expansion is reported at the macro call. Without it such errors point into UVM, and when UVM is a library path (or `report_library_macro_expansions` is off) they are dropped, hiding a real compile error. |
| `slang-compat-vcs` | vcs | Turns on slang's own `--compat vcs`. |
| `error-taint` | none | When a task or function has a compile error, slang keeps what it can of the body, and which parts survive changes between slang releases; rules reading that body can report things that are not there. Findings inside such a subroutine become Info with a note naming the error line. Findings on the error's own line keep their severity, since those rules point at the illegal construct. The cost: a real finding in a broken subroutine is downgraded too until the error is fixed. |

## Adding an override

An override is a class derived from `compat::Override`
(`include/uvmcheck/compat/Overlay.h`) registered in `src/compat/Overlay.cpp`.
It hooks into one stage:

- `configureDriver`: after the command line and project config are read, before
  slang processes its options and loads sources (include dirs, extra sources,
  slang options, library paths);
- `configureCompilation`: after the compilation is created, before anything
  elaborates (`Compilation::addSystemSubroutine` and similar);
- `reviewFindings`: after the rules ran, before waivers and baselines apply.

It uses only slang's public API. When it stands in for a slang bug, it names the
upstream issue (`upstream()`) and the first slang major version with the fix
(`retireAtMajor()`). From that version on it is skipped, and the overlay test
fails until the override is deleted, so workarounds do not outlive their bug.

Overrides never silence a real IEEE 1800 error by default; turning one of
slang's errors off stays an explicit `-Wno-...` choice.
