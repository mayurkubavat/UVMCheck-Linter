// UVM-RUS-002: compliant code.
`include "uvm_mock.svh"
class bus_agent_cfg extends uvm_object;
  `uvm_object_utils(bus_agent_cfg)
  int unsigned addr_width;
  bit has_coverage;
  string mode;
  virtual bus_if vif;
  function new(string name = "bus_agent_cfg");
    super.new(name);
  endfunction
endclass

class bus_agent extends uvm_agent;
  `uvm_component_utils(bus_agent)
  bus_agent_cfg cfg;
  bit verbose;
  function new(string name, uvm_component parent);
    super.new(name, parent);
  endfunction
  function void build_phase(uvm_phase phase);
    super.build_phase(phase);
    void'(uvm_config_db#(bus_agent_cfg)::get(this, "", "cfg", cfg));
    void'(uvm_config_db#(bit)::get(this, "", "verbose", verbose));
  endfunction
endclass
