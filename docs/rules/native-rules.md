# Rules only dv-lint has

Most dv-lint rules are ports of uvmcheck's C++ rules and are listed in
[README.md](README.md) (generated from the C++ binary). The rules below have no
C++ twin: `dv-lint --list` marks them `native`, and the parity gates
(`tools/gates/lint_parity.py`) leave them out.

| Rule | Name | Severity | What it flags |
|---|---|---|---|
| `UVM-API-003` | DeprecatedUvmApi | WARNING | Calls to UVM methods, and classes extending UVM classes, that the compiled UVM release keeps only as deprecated API (UVM 1.1d/1.2: what `UVM_NO_DEPRECATED` removes, such as `uvm_component::set_config_int` or `uvm_random_sequence`). Read from the release's API pack in `data/uvm-api/`. |
