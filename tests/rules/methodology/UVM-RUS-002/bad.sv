// UVM-RUS-002: violations are marked.
`include "uvm_mock.svh"
class bus_agent extends uvm_agent; // agent
  `uvm_component_utils(bus_agent)
  int unsigned addr_width;
  bit has_coverage;
  string mode;
  virtual bus_if vif;
  function new(string name, uvm_component parent);
    super.new(name, parent);
  endfunction
  function void build_phase(uvm_phase phase);
    super.build_phase(phase);
    void'(uvm_config_db#(int unsigned)::get(this, "", "addr_width", addr_width)); // VIOLATION
    void'(uvm_config_db#(bit)::get(this, "", "has_coverage", has_coverage));
    void'(uvm_config_db#(string)::get(this, "", "mode", mode));
    void'(uvm_config_db#(virtual bus_if)::get(this, "", "vif", vif));
  endfunction
endclass
