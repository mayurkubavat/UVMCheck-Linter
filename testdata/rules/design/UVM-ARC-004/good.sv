// UVM-ARC-004 good: tests and end_of_elaboration checks may look things up.
`include "uvm_mock.svh"

class bus_monitor extends uvm_monitor;
  `uvm_component_utils(bus_monitor)
  uvm_component scb;
  function new(string name, uvm_component parent);
    super.new(name, parent);
  endfunction
  function void end_of_elaboration_phase(uvm_phase phase);
    scb = uvm_top.find("uvm_test_top.env.scb");
  endfunction
endclass

class base_test extends uvm_test;
  `uvm_component_utils(base_test)
  uvm_component env;
  function new(string name, uvm_component parent);
    super.new(name, parent);
  endfunction
  task run_phase(uvm_phase phase);
    env = uvm_top.find("uvm_test_top.env");
  endtask
endclass
