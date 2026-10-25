# MG-3 Cohesion and component roles

Grounded in the single responsibility principle, GRASP information expert, class cohesion (LCOM) and Fowler's Feature Envy smell.

UVM gives every component a role. Keeping each class to its role is what makes an agent usable in passive mode, a scoreboard replaceable, and a test short.

## MG-3.1 (Shall)

**Each UVM role does one job: sequences generate, drivers drive, monitors observe, scoreboards check, subscribers cover.**

A driver that randomizes its own items cannot be steered by tests; coverage sampled in a driver records intent, not what reached the DUT; a monitor that holds a driver handle fails in passive agents.

Instead of:

```systemverilog
task bus_driver::run_phase(uvm_phase phase);
  void'(req.randomize());        // stimulus in the driver
endtask
```

Write:

```systemverilog
task bus_driver::run_phase(uvm_phase phase);
  seq_item_port.get_next_item(req);
  drive(req);
  seq_item_port.item_done();
endtask
```

Enforced by: `UVM-ROL-001`, `UVM-ROL-002`, `UVM-ROL-005`, `UVM-SEQ-001`, `UVM-VIF-DIR`, `UVM-OBS-001`, `UVM-TLM-005`, `UVM-ALS-001`, `SV-DSN-002`, `SV-DSN-003`, `SV-MET-006`

## MG-3.2 (Should)

**Agents carry protocol knowledge only; DUT-specific checking lives in the env.**

Scoreboards and reference models are specific to one DUT. Inside an agent they stop the agent being reused with another DUT.

Instead of:

```systemverilog
class bus_agent extends uvm_agent;
  bus_scoreboard sb;
endclass
```

Write:

```systemverilog
class soc_env extends uvm_env;
  bus_agent      agt;
  bus_scoreboard sb;   // connected to agt.mon.ap
endclass
```

Enforced by: `UVM-ROL-003`

## MG-3.3 (Should)

**Tests configure and select; envs build.**

When each test builds its own drivers and agents, tests drift apart. An env builds the structure once; tests change it through config objects and factory overrides.

Enforced by: `UVM-ROL-004`
