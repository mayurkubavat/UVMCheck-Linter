// UVM-CFG-009: violations are marked.
`include "uvm_mock.svh"
class bus_cfg extends uvm_object;
  `uvm_object_utils(bus_cfg)
  int timeout;
  bit enable_checks;
  function new(string name = "bus_cfg");
    super.new(name);
  endfunction
endclass

class bus_scoreboard extends uvm_scoreboard;
  `uvm_component_utils(bus_scoreboard)
  bus_cfg cfg;
  function new(string name, uvm_component parent);
    super.new(name, parent);
  endfunction
  function void build_phase(uvm_phase phase);
    void'(uvm_config_db#(bus_cfg)::get(this, "", "cfg", cfg));
    cfg.timeout = 100;
  endfunction
  task run_phase(uvm_phase phase);
    cfg.enable_checks = 0; // VIOLATION
  endtask
  function void error_seen();
    cfg.timeout = 0; // VIOLATION
  endfunction
endclass
