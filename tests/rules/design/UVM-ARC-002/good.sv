// UVM-ARC-002 good: picking a sequencer, wiring in connect_phase, one level of access.
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

class bus_seq extends uvm_sequence;
  `uvm_object_utils(bus_seq)
  function new(string name = "bus_seq");
    super.new(name);
  endfunction
endclass

class base_test extends uvm_test;
  `uvm_component_utils(base_test)
  soc_env env;
  bus_sequencer target;
  function new(string name, uvm_component parent);
    super.new(name, parent);
  endfunction

  function void connect_phase(uvm_phase phase);
    target = env.agt[0].sqr;
    env.agt[1].drv.vif = env.agt[0].drv.vif;
  endfunction

  task run_phase(uvm_phase phase);
    bus_seq seq = new("seq");
    seq.start(env.agt[0].sqr);
  endtask
endclass
