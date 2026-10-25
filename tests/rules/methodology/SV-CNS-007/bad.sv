// SV-CNS-007: violations are marked.
`include "uvm_mock.svh"
class bus_scoreboard extends uvm_scoreboard;
  `uvm_component_utils(bus_scoreboard)
  integer n_match; // VIOLATION
  function new(string name, uvm_component parent);
    super.new(name, parent);
  endfunction
  function integer count(integer limit); // VIOLATION VIOLATION
    integer i; // VIOLATION
    return n_match;
  endfunction
endclass
