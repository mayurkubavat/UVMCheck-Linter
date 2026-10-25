# MG-2 Coupling

Grounded in Myers' coupling scale (content, common, control, stamp, data, from worst to best), Page-Jones's connascence, the Law of Demeter and the dependency inversion principle.

Coupling is how much one class must know about another to work. In a UVM testbench the cheapest couplings are TLM ports, config objects and sequencer handles; the most expensive are writing another component's fields, reaching through the hierarchy and sharing global state.

## MG-2.1 (Shall)

**Components talk through TLM ports, config objects and sequences; they do not hold handles to envs and tests or write each other's state.**

Writing another component's fields at run time is content coupling: the owner can no longer change how it keeps its state, and the write is invisible when reading the owner's code. Holding an env or test handle makes a class depend on everything the env contains.

Instead of:

```systemverilog
task soc_env::run_phase(uvm_phase phase);
  agt.drv.enabled = 0;           // writes the driver's state
endtask
```

Write:

```systemverilog
// the driver reads its config object, or offers a method
agt_cfg.drive_enable = 0;
```

Enforced by: `UVM-CPL-002`, `UVM-CPL-003`, `UVM-ARC-002`, `UVM-SEQ-003`, `UVM-SEQ-011`, `UVM-PHS-010`, `SV-ENC-001`, `SV-DSN-004`

## MG-2.2 (Shall)

**Class code never names a module hierarchy; virtual interfaces arrive through configuration.**

A class that names tb_top.dut.sig only works under that one testbench top. A virtual interface from config_db keeps the class independent of where it is instantiated.

Instead of:

```systemverilog
@(posedge tb_top.u_dut.irq);
```

Write:

```systemverilog
@(posedge vif.irq);   // vif from uvm_config_db
```

Enforced by: `UVM-CPL-004`, `SV-OOP-008`

## MG-2.3 (Should)

**Shared state is passed down at build time, not kept in globals, statics, the command line or run-time hierarchy lookups.**

Statics, package variables, plusargs read deep inside components and find() lookups are all hidden inputs. Passing state down at build time keeps every dependency visible in the constructor or config object.

config_db is the build-time mechanism for this. Called from run-time phases or inside a task's loop it becomes a slow global mailbox (UVM-CFG-007), and a config object fetched from it is shared by everyone who read it, so only its owner should change it after the build (UVM-CFG-009).

Instead of:

```systemverilog
task bus_driver::run_phase(uvm_phase phase);
  forever begin
    void'(uvm_config_db#(int)::get(this, "", "delay", delay));
    cfg.enable_checks = 0;     // cfg came from config_db
  end
endtask
```

Write:

```systemverilog
function void bus_driver::build_phase(uvm_phase phase);
  void'(uvm_config_db#(bus_cfg)::get(this, "", "cfg", cfg));
endfunction
// run-time changes come through a TLM port, an event or a method on cfg
```

Enforced by: `SV-DSN-008`, `SV-OOP-007`, `UVM-CFG-003`, `UVM-CFG-005`, `UVM-CFG-006`, `UVM-ARC-004`, `UVM-CFG-007`, `UVM-CFG-009`

## MG-2.4 (Should)

**Names and positions that tie code together (config keys, value types, argument order) match on both sides and are checked, not left to convention.**

When two pieces of code must agree on a name, type or argument order, the agreement should be checked by the compiler or a rule, not by convention.

A config_db field name typed as a literal in several files (UVM-CFG-008) and a routine that picks its behaviour from a string argument (SV-CPL-001) both depend on matching spellings that nothing checks.

Instead of:

```systemverilog
uvm_config_db#(bus_cfg)::set(this, "agt", "bus_cfg", cfg);       // in the env
void'(uvm_config_db#(bus_cfg)::get(this, "", "bus_cfg", cfg));   // in the agent
seq.do_op("write");
```

Write:

```systemverilog
localparam string BUS_CFG_KEY = "bus_cfg";   // in bus_pkg, used on both sides
seq.do_op(OP_WRITE);                          // enum argument
```

Enforced by: `UVM-CFG-001`, `UVM-DAG-001`, `SV-RDB-001`, `SV-BUG-007`, `UVM-CFG-008`, `SV-CPL-001`
