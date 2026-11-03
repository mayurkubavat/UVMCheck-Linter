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
