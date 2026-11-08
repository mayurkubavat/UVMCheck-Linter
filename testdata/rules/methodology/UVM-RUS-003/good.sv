// UVM-RUS-003: compliant code.
`include "uvm_mock.svh"
class bus_agent extends uvm_agent;
  `uvm_component_utils(bus_agent)
  function new(string name, uvm_component parent);
    super.new(name, parent);
  endfunction
endclass

class soc_env_cfg extends uvm_object;
  `uvm_object_utils(soc_env_cfg)
  int unsigned num_agents = 4;
  function new(string name = "soc_env_cfg");
    super.new(name);
  endfunction
endclass

class soc_env extends uvm_env;
  `uvm_component_utils(soc_env)
  soc_env_cfg cfg;
  bus_agent agt[$];
  function new(string name, uvm_component parent);
    super.new(name, parent);
  endfunction
  function void build_phase(uvm_phase phase);
    super.build_phase(phase);
    void'(uvm_config_db#(soc_env_cfg)::get(this, "", "cfg", cfg));
    for (int i = 0; i < cfg.num_agents; i++) begin
      agt.push_back(bus_agent::type_id::create($sformatf("agt%0d", i), this));
    end
  endfunction
endclass
