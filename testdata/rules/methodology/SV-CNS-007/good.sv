// SV-CNS-007: int and sized vectors in classes, and integer in modules, are fine.
`include "uvm_mock.svh"
class bus_scoreboard extends uvm_scoreboard;
  `uvm_component_utils(bus_scoreboard)
  int n_match;
  logic [31:0] last;
  function new(string name, uvm_component parent);
    super.new(name, parent);
  endfunction
endclass

module tb;
  integer fd;
endmodule
