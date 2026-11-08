// UVM-ROL-003: compliant code.
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
  function new(string name, uvm_component parent);
    super.new(name, parent);
  endfunction
endclass

class soc_env extends uvm_env;
  `uvm_component_utils(soc_env)
  bus_agent agt;
  bus_scoreboard sb;
  function new(string name, uvm_component parent);
    super.new(name, parent);
  endfunction
endclass
