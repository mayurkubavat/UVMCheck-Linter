// UVM-ARC-002: the test pokes three levels down into the env.
`include "uvm_mock.svh"

interface tb_bus_if;
  logic ready;
endinterface

class bus_driver extends uvm_component;
  `uvm_component_utils(bus_driver)
  virtual tb_bus_if vif;
  function new(string name, uvm_component parent);
    super.new(name, parent);
  endfunction
endclass

class bus_sequencer extends uvm_sequencer;
  `uvm_component_utils(bus_sequencer)
  function new(string name, uvm_component parent);
    super.new(name, parent);
  endfunction
endclass

class bus_agent extends uvm_agent;
  `uvm_component_utils(bus_agent)
  bus_driver drv;
  bus_sequencer sqr;
  int unsigned pending;
  function new(string name, uvm_component parent);
    super.new(name, parent);
  endfunction
endclass

class soc_env extends uvm_env;
  `uvm_component_utils(soc_env)
  bus_agent agt[2];
  function new(string name, uvm_component parent);
    super.new(name, parent);
  endfunction
endclass

class base_test extends uvm_test;
  `uvm_component_utils(base_test)
  soc_env env;
  function new(string name, uvm_component parent);
    super.new(name, parent);
  endfunction

  task run_phase(uvm_phase phase);
    env.agt[0].drv.vif.ready = 1; // VIOLATION
  endtask
endclass
