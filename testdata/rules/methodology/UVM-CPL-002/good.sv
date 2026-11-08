// UVM-CPL-002: compliant code.
`include "uvm_mock.svh"
class soc_env extends uvm_env;
  `uvm_component_utils(soc_env)
  int unsigned errors;
  function new(string name, uvm_component parent);
    super.new(name, parent);
  endfunction
endclass

class bus_cfg extends uvm_object;
  `uvm_object_utils(bus_cfg)
  int unsigned max_len;
  function new(string name = "bus_cfg");
    super.new(name);
  endfunction
endclass

class bus_seq extends uvm_sequence_item;
  `uvm_object_utils(bus_seq)
  bus_cfg cfg;
  function new(string name = "bus_seq");
    super.new(name);
  endfunction
endclass

// An env may hold its sub-envs.
class top_env extends uvm_env;
  `uvm_component_utils(top_env)
  soc_env sub;
  function new(string name, uvm_component parent);
    super.new(name, parent);
  endfunction
endclass
