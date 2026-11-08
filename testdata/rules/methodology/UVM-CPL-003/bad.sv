// UVM-CPL-003: violations are marked.
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
  task run_phase(uvm_phase phase);
    agt.drv.enabled = 0; // VIOLATION
  endtask
  function void start_of_simulation_phase(uvm_phase phase);
    agt.drv.delay = 5; // VIOLATION
  endfunction
endclass
