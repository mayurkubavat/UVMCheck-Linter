# MG-5 Design-pattern conformance

Grounded in the Gang of Four patterns as UVM uses them: factory, template method (phases), observer (analysis ports), strategy, adapter (register adapters) and callbacks.

UVM is built from design patterns. Each pattern only works when all of its parts are present, and variation should flow through its extension points rather than through type switches.

## MG-5.1 (Shall)

**When a pattern's mechanism is used, all of its parts are present: callbacks are registered, adapters are complete, ports and sequencers are wired.**

A callback that is not registered is never called; an adapter that never sets rw.status reports every access as passed; an unconnected port silently drops traffic.

Instead of:

```systemverilog
`uvm_do_callbacks(bus_driver, bus_driver_cb, pre_drive())
// no `uvm_register_cb(bus_driver, bus_driver_cb)
```

Write:

```systemverilog
class bus_driver extends uvm_driver #(bus_item);
  `uvm_register_cb(bus_driver, bus_driver_cb)
  ...
endclass
```

Enforced by: `UVM-PAT-001`, `RAL-ADP-001`, `RAL-WIRE-001`, `UVM-VSEQ-001`, `UVM-TLM-001`, `UVM-TLM-004`

## MG-5.2 (Should)

**Behaviour varies through overrides, hooks and callbacks, not type switches or edits to the base class; overrides keep the base contract.**

A ladder of $cast or get_type_name() comparisons must be edited for every new type. Virtual methods, factory overrides and callbacks let new behaviour be added without touching the base.

Enforced by: `SV-DSN-001`, `SV-DSN-009`, `SV-OCP-001`, `SV-DSN-006`, `SV-DSN-007`, `SV-OOP-001`, `SV-OOP-009`, `SV-OOP-015`, `SV-LSP-001`, `SV-RDB-004`

## MG-5.3 (Should)

**No hand-rolled singletons; shared services go through the config or resource database.**

A hand-rolled singleton is a global variable with extra steps: hidden, shared by every env instance and hard to replace in a test.

Enforced by: review only (no rule yet).
