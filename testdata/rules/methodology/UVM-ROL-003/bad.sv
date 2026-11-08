// UVM-ROL-003: violations are marked.
`include "uvm_mock.svh"
class bus_scoreboard extends uvm_scoreboard;
  `uvm_component_utils(bus_scoreboard)
  function new(string name, uvm_component parent);
    super.new(name, parent);
  endfunction
endclass

class bus_monitor extends uvm_monitor;
  `uvm_component_utils(bus_monitor)
  function new(string name, uvm_component parent);
    super.new(name, parent);
  endfunction
endclass

class bus_agent extends uvm_agent;
  `uvm_component_utils(bus_agent)
  bus_monitor mon;
  bus_scoreboard sb; // VIOLATION
  function new(string name, uvm_component parent);
    super.new(name, parent);
  endfunction
endclass
