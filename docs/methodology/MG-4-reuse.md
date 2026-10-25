# MG-4 Reusable verification components

Grounded in the open/closed principle and the UVM idea of horizontal (across projects) and vertical (block to system) reuse.

A reusable component is changed by configuring it, not by editing it. That needs active/passive support, a config object, type-safe factory use and items that carry their own copy, compare and print.

## MG-4.1 (Shall)

**Every agent works in both active and passive mode.**

A larger env reuses a block-level agent as a passive monitor. If the agent always builds its driver, the driver fights the real bus master.

Instead of:

```systemverilog
drv = bus_driver::type_id::create("drv", this);
```

Write:

```systemverilog
if (get_is_active() == UVM_ACTIVE) begin
  drv = bus_driver::type_id::create("drv", this);
  sqr = bus_sequencer::type_id::create("sqr", this);
end
```

Enforced by: `UVM-RUS-001`

## MG-4.2 (Shall)

**Agent and env behaviour is controlled by a config object, not by literals or scattered config_db fields.**

Settings that travel together belong in one typed, documented, randomizable config object, set with one call. Literal counts in an env fix its topology for every user.

Instead of:

```systemverilog
void'(uvm_config_db#(int)::get(this, "", "addr_width", addr_width));
void'(uvm_config_db#(bit)::get(this, "", "has_cov", has_cov));
void'(uvm_config_db#(string)::get(this, "", "mode", mode));
```

Write:

```systemverilog
void'(uvm_config_db#(bus_agent_cfg)::get(this, "", "cfg", cfg));
```

Enforced by: `UVM-RUS-002`, `UVM-RUS-003`

## MG-4.3 (Should)

**Overrides and lookups use types, not strings; config scopes are relative; nothing names a host path or a vendor-only call.**

String type names are only checked when the test runs. Absolute config paths tie a class to one place in one test's hierarchy; with a non-null context they never match at all.

Instead of:

```systemverilog
set_type_override("bus_driver", "slow_driver");
```

Write:

```systemverilog
bus_driver::type_id::set_type_override(slow_driver::get_type());
```

Enforced by: `UVM-RUS-004`, `UVM-RUS-005`, `PORT-PTH-001`, `PORT-PRP-001`

## MG-4.4 (Should)

**Sequence items carry their full object contract: copy, compare and print.**

Without do_copy, do_compare and convert2string, clone() returns an empty copy, compare() always passes and logs print nothing useful.

Enforced by: `UVM-RUS-006`, `SV-BUG-004`, `UVM-SEQ-008`

## MG-4.5 (Shall)

**Every reusable class is registered with the factory and created through it.**

Only factory-created classes can be replaced by tests. Direct new() and missing registration remove that extension point.

Enforced by: `UVM-FAC-001`, `UVM-FAC-002`, `UVM-FAC-003`, `UVM-FAC-004`, `UVM-STR-005`, `UVM-SEQ-010`
