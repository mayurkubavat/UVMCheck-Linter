// UVM-RUS-003: violations are marked.
`include "uvm_mock.svh"
class bus_agent extends uvm_agent;
  `uvm_component_utils(bus_agent)
  function new(string name, uvm_component parent);
    super.new(name, parent);
  endfunction
endclass

class soc_env extends uvm_env;
  `uvm_component_utils(soc_env)
  bus_agent agt[$];
  function new(string name, uvm_component parent);
    super.new(name, parent);
  endfunction
  function void build_phase(uvm_phase phase);
    super.build_phase(phase);
    for (int i = 0; i < 4; i++) begin // VIOLATION
      agt.push_back(bus_agent::type_id::create($sformatf("agt%0d", i), this));
    end
  endfunction
endclass
