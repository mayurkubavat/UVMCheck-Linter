# MG-1 Layering and packages

Grounded in Robert C. Martin's package principles (reuse/release equivalence, common reuse, acyclic and stable dependencies) and Parnas's information hiding.

A testbench is a stack of layers. Items and sequences are the most reused; tests the least. Each layer may use the ones below it, never the ones above, and each package is a unit that can be compiled and reused on its own.

## MG-1.1 (Shall)

**Dependencies point down the testbench stack: item, sequence, agent, env, test. Nothing lower knows about anything higher.**

A sequence that names a scoreboard, or an agent that names its env, cannot be compiled without that higher layer, so it cannot be reused anywhere else.

Instead of:

```systemverilog
class bus_seq extends uvm_sequence #(bus_item);
  soc_scoreboard sb;   // sequence knows the env's checker
endclass
```

Write:

```systemverilog
class bus_seq extends uvm_sequence #(bus_item);
  bus_seq_cfg cfg;     // only what the sequence needs
endclass
```

Enforced by: `UVM-ARC-001`

## MG-1.2 (Should)

**One class per file, and the file is named after the class.**

A reader finds class bus_driver in bus_driver.svh without searching. One class per file also keeps diffs and merge conflicts small.

Enforced by: review only (no rule yet).

## MG-1.3 (Shall)

**Each file belongs to one package; declarations, imports and macros stay inside it and do not leak into $unit or other files.**

A file `include'd into two packages creates two incompatible types with the same name. Declarations at $unit scope and macros that are never undefined leak into every file compiled after them.

Instead of:

```systemverilog
// bus_item.svh included by both bus_pkg and soc_pkg
`include "bus_item.svh"
```

Write:

```systemverilog
package bus_pkg;
  `include "bus_item.svh"
endpackage
// soc_pkg imports bus_pkg::bus_item instead
```

Enforced by: `UVM-ARC-003`, `SV-MOD-001`, `SV-MAC-001`, `SV-STY-013`

## MG-1.4 (May)

**Packages that many others depend on stay small and stable.**

A package many others import should change rarely, because every change ripples into its dependents. Put volatile, project-specific code in packages that nothing else depends on.

Enforced by: review only (no rule yet).
