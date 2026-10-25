# MG-6 Construct usage

Grounded in restricted language subsets such as MISRA C, NASA's Power of Ten and the SEI CERT standards, and in Fowler's Primitive Obsession smell.

SystemVerilog has several ways to do most things. The guide prefers the typed, current, compiler-checked one.

## MG-6.1 (Shall)

**Prefer typed language constructs to the preprocessor and to code-generating macros.**

Macros are untyped and invisible to the debugger. `uvm_field_* and `uvm_do hide large amounts of generated code and are slow.

Enforced by: `UVM-BAN-001`, `UVM-BAN-002`

## MG-6.2 (Should)

**Model domain values with types: enums instead of strings, packed structs instead of hand slicing.**

A string compared against a list of literals is an enum the compiler cannot check; hand-written bit slices are fields that have no names.

Enforced by: review only (no rule yet).

## MG-6.3 (Shall)

**Use current language and UVM constructs; deprecated and legacy forms are not used.**

Deprecated constructs (program blocks, defparam, $random, set_config_*) are removed or discouraged by IEEE 1800 and 1800.2 and behave differently between tools.

Enforced by: `SV-DEP-001`, `SV-DEP-002`, `SV-DEP-003`, `SV-DEP-004`, `SV-DEP-005`, `SV-DEP-006`, `UVM-DEP-001`, `UVM-DEP-002`, `UVM-DEP-003`, `UVM-DEP-004`, `SV-BAN-001`, `SV-COR-009`
