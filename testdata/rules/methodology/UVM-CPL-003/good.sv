// UVM-CPL-003: compliant code.
`include "uvm_mock.svh"
class bus_driver extends uvm_component;
  `uvm_component_utils(bus_driver)
  int unsigned delay;
  bit enabled = 1;
  function new(string name, uvm_component parent);
    super.new(name, parent);
  endfunction
endclass

class bus_agent extends uvm_agent;
  `uvm_component_utils(bus_agent)
  bus_driver drv;
  function new(string name, uvm_component parent);
    super.new(name, parent);
  endfunction
endclass

class soc_env extends uvm_env;
  `uvm_component_utils(soc_env)
  bus_agent agt;
  function new(string name, uvm_component parent);
    super.new(name, parent);
  endfunction
  function void build_phase(uvm_phase phase);
    super.build_phase(phase);
    agt = bus_agent::type_id::create("agt", this);
    // A parent setting up its own child while building is wiring.
    agt.is_active = UVM_PASSIVE;
  endfunction
  function void connect_phase(uvm_phase phase);
    agt.drv.delay = 5;
  endfunction
endclass
