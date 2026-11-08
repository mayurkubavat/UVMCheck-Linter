// UVM-ARC-004: a monitor finds the scoreboard by path string.
`include "uvm_mock.svh"

class bus_scoreboard extends uvm_scoreboard;
  `uvm_component_utils(bus_scoreboard)
  function new(string name, uvm_component parent);
    super.new(name, parent);
  endfunction
endclass

class bus_monitor extends uvm_monitor;
  `uvm_component_utils(bus_monitor)
  uvm_component scb;
  uvm_component all[$];
  function new(string name, uvm_component parent);
    super.new(name, parent);
  endfunction
  task run_phase(uvm_phase phase);
    scb = uvm_top.find("uvm_test_top.env.scb"); // VIOLATION
    uvm_top.find_all("*.agt*", all); // VIOLATION
  endtask
endclass
