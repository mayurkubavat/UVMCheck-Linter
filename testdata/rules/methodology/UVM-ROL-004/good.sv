// UVM-ROL-004: compliant code.
`include "uvm_mock.svh"
class bus_agent extends uvm_agent;
  `uvm_component_utils(bus_agent)
  function new(string name, uvm_component parent);
    super.new(name, parent);
  endfunction
endclass

class bus_scoreboard extends uvm_scoreboard;
  `uvm_component_utils(bus_scoreboard)
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
  function void build_phase(uvm_phase phase);
    super.build_phase(phase);
    agt = bus_agent::type_id::create("agt", this);
    sb = bus_scoreboard::type_id::create("sb", this);
  endfunction
endclass

class smoke_test extends uvm_test;
  `uvm_component_utils(smoke_test)
  soc_env env;
  function new(string name, uvm_component parent);
    super.new(name, parent);
  endfunction
  function void build_phase(uvm_phase phase);
    super.build_phase(phase);
    env = soc_env::type_id::create("env", this);
  endfunction
endclass
