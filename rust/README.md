# Rust front end (R0)

The first step of the plan to give every DV tool one design model, with slang
moving behind it layer by layer.

```
tools (dv-map, later dv-lsp, dv-fmt, ...)
  └─ dv-model     safe, owned Rust snapshot of the design
       └─ sv-slang   raw FFI to libuvmcheck_model (C ABI)
            └─ uvmcheck::model  (C++ facade, include/uvmcheck/model/)
                 └─ slang
```

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

## Why a C ABI and not cxx

The plan named `cxx`. A plain C ABI does the same job for Rust with no code
generation, and also serves Python (`ctypes`), C tools and any future language,
which is the point of a central library. `cxx` can still be layered on later.
