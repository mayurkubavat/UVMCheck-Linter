// UVM-RUS-004: violations are marked.
`include "uvm_mock.svh"
class bus_agent extends uvm_agent;
  `uvm_component_utils(bus_agent)
  virtual bus_if vif;
  function new(string name, uvm_component parent);
    super.new(name, parent);
  endfunction
  function void build_phase(uvm_phase phase);
    super.build_phase(phase);
    void'(uvm_config_db#(virtual bus_if)::get(null, "uvm_test_top.env.agt", "vif", vif)); // VIOLATION
    uvm_config_db#(int)::set(this, "uvm_test_top.env.agt.drv", "delay", 3); // VIOLATION
  endfunction
endclass
