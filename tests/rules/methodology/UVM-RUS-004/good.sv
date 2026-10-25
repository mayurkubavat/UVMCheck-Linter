// UVM-RUS-004: compliant code.
`include "uvm_mock.svh"
class bus_agent extends uvm_agent;
  `uvm_component_utils(bus_agent)
  virtual bus_if vif;
  function new(string name, uvm_component parent);
    super.new(name, parent);
  endfunction
  function void build_phase(uvm_phase phase);
    super.build_phase(phase);
    void'(uvm_config_db#(virtual bus_if)::get(this, "", "vif", vif));
    uvm_config_db#(int)::set(this, "drv", "delay", 3);
  endfunction
endclass

// The test sits at uvm_test_top, so absolute paths from it are what the top module would use.
class base_test extends uvm_test;
  `uvm_component_utils(base_test)
  function new(string name, uvm_component parent);
    super.new(name, parent);
  endfunction
  function void build_phase(uvm_phase phase);
    super.build_phase(phase);
    uvm_config_db#(int)::set(null, "uvm_test_top.env*", "delay", 3);
  endfunction
endclass
