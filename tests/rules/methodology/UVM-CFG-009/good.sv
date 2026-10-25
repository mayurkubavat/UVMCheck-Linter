// UVM-CFG-009: the owner changes its own config object; others only read it.
`include "uvm_mock.svh"
class bus_cfg extends uvm_object;
  `uvm_object_utils(bus_cfg)
  int timeout;
  function new(string name = "bus_cfg");
    super.new(name);
  endfunction
endclass

class bus_test extends uvm_test;
  `uvm_component_utils(bus_test)
  bus_cfg cfg;
  function new(string name, uvm_component parent);
    super.new(name, parent);
  endfunction
  function void build_phase(uvm_phase phase);
    cfg = bus_cfg::type_id::create("cfg");
    uvm_config_db#(bus_cfg)::set(this, "*", "cfg", cfg);
  endfunction
  task run_phase(uvm_phase phase);
    cfg.timeout = 5;
  endtask
endclass

class bus_driver extends uvm_driver;
  `uvm_component_utils(bus_driver)
  bus_cfg cfg;
  function new(string name, uvm_component parent);
    super.new(name, parent);
  endfunction
  function void build_phase(uvm_phase phase);
    void'(uvm_config_db#(bus_cfg)::get(this, "", "cfg", cfg));
    apply_defaults();
  endfunction
  function void apply_defaults();
    cfg.timeout = 10;
    void'(uvm_config_db#(int)::get(this, "", "timeout", cfg.timeout));
  endfunction
endclass

class bus_monitor extends uvm_monitor;
  `uvm_component_utils(bus_monitor)
  bus_cfg cfg;
  int t;
  function new(string name, uvm_component parent);
    super.new(name, parent);
  endfunction
  function void build_phase(uvm_phase phase);
    void'(uvm_config_db#(bus_cfg)::get(this, "", "cfg", cfg));
  endfunction
  task run_phase(uvm_phase phase);
    t = cfg.timeout;
  endtask
endclass
