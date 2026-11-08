// SV-CNS-008: violations are marked.
`include "uvm_mock.svh"
class bus_model extends uvm_component;
  `uvm_component_utils(bus_model)
  function new(string name, uvm_component parent);
    super.new(name, parent);
  endfunction
  function void predict(int addr, output int data); // VIOLATION
    data = addr * 2;
  endfunction
  function void decode(int raw, output int kind, output bit err); // VIOLATION
    kind = raw;
    err = 0;
  endfunction
endclass
