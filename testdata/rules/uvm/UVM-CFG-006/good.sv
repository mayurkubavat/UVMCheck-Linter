// UVM-CFG-006 good: the test reads plusargs into the config object.
`include "uvm_mock.svh"

class bus_cfg extends uvm_object;
  `uvm_object_utils(bus_cfg)
  int unsigned delay;
  function new(string name = "bus_cfg");
    super.new(name);
  endfunction
  function void apply_plusargs();
    void'($value$plusargs("BUS_DELAY=%d", delay));
  endfunction
endclass

class base_test extends uvm_test;
  `uvm_component_utils(base_test)
  bus_cfg cfg;
  bit slow;
  function new(string name, uvm_component parent);
    super.new(name, parent);
  endfunction
  function void build_phase(uvm_phase phase);
    slow = $test$plusargs("SLOW_BUS");
  endfunction
endclass
