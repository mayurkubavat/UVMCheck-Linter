# MG-6 Construct usage

Grounded in restricted language subsets such as MISRA C, NASA's Power of Ten and the SEI CERT standards, and in Fowler's Primitive Obsession smell.

SystemVerilog has several ways to do most things. The guide prefers the typed, current, compiler-checked one.

## MG-6.1 (Shall)

**Prefer typed language constructs to the preprocessor and to code-generating macros.**

Macros are untyped and invisible to the debugger. `uvm_field_* and `uvm_do hide large amounts of generated code and are slow.

A `define that only holds a number or a string is a constant without a type or scope (SV-CNS-001); a package parameter does the same job. A code macro that uses nothing only the preprocessor can do (token pasting, stringification, `__FILE__, type arguments, declarations) is a routine without type checking (SV-CNS-002). `ifdef inside a class turns one class into several that must each be built and tested, and cannot be chosen per instance (SV-CNS-003). Defaults wrapped in `ifndef NAME, which the build overrides with +define+, and simulator or UVM version shims are left alone.

Instead of:

```systemverilog
`define BUS_WIDTH 32
class bus_monitor extends uvm_monitor;
`ifdef BUS_COVERAGE
  bus_cov cov;
`endif
endclass
```

Write:

```systemverilog
package bus_pkg;
  localparam int BUS_WIDTH = 32;
endpackage
class bus_monitor extends uvm_monitor;
  bus_cfg cfg;     // cfg.has_coverage decides whether cov is built
  bus_cov cov;
endclass
```

Enforced by: `UVM-BAN-001`, `UVM-BAN-002`, `SV-CNS-001`, `SV-CNS-002`, `SV-CNS-003`

## MG-6.2 (Should)

**Model domain values with types: enums instead of strings, packed structs instead of hand slicing.**

A string compared against a list of literals is an enum the compiler cannot check (SV-CNS-004); hand-written bit slices are fields that have no names (SV-CNS-005). Strings that come from a UVM API, such as register access policies from get_access(), are left alone.

Instead of:

```systemverilog
case (mode)
  "read": ...  "write": ...  "idle": ...
endcase
len  = hdr[15:0];
kind = hdr[23:16];
```

Write:

```systemverilog
typedef enum {READ, WRITE, IDLE} mode_e;
typedef struct packed { logic [7:0] flags; logic [7:0] kind; logic [15:0] len; } hdr_t;
len = hdr.len;
```

Enforced by: `SV-CNS-004`, `SV-CNS-005`

## MG-6.3 (Shall)

**Use current, explicit language and UVM constructs; deprecated and legacy forms are not used.**

Deprecated constructs (program blocks, defparam, $random, set_config_*) are removed or discouraged by IEEE 1800 and 1800.2 and behave differently between tools.

Three Advisory rules ask for explicit types: an enum without a base type is a 32-bit int (SV-CNS-006), the Verilog-1995 integer type is 4-state and signed (SV-CNS-007), and a void function that hands back results through output arguments hides its data flow (SV-CNS-008).

Enforced by: `SV-DEP-001`, `SV-DEP-002`, `SV-DEP-003`, `SV-DEP-004`, `SV-DEP-005`, `SV-DEP-006`, `UVM-DEP-001`, `UVM-DEP-002`, `UVM-DEP-003`, `UVM-DEP-004`, `SV-BAN-001`, `SV-COR-009`, `SV-CNS-006`, `SV-CNS-007`, `SV-CNS-008`
