// UVM-CPL-002: violations are marked.
`include "uvm_mock.svh"
class soc_env extends uvm_env;
  `uvm_component_utils(soc_env)
  int unsigned errors;
  function new(string name, uvm_component parent);
    super.new(name, parent);
  endfunction
endclass

class bus_seq extends uvm_sequence_item;
  `uvm_object_utils(bus_seq)
  soc_env env; // VIOLATION
  function new(string name = "bus_seq");
    super.new(name);
  endfunction
endclass
