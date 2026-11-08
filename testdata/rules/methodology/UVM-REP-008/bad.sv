// UVM-REP-008: violations are marked.
`include "uvm_mock.svh"
class bus_item extends uvm_sequence_item;
  `uvm_object_utils(bus_item)
  bit [31:0] data;
  function new(string name = "bus_item");
    super.new(name);
  endfunction
endclass

class bus_scoreboard extends uvm_scoreboard;
  `uvm_component_utils(bus_scoreboard)
  bus_item expected[$];
  function new(string name, uvm_component parent);
    super.new(name, parent);
  endfunction
  function void write(bus_item actual);
    bus_item exp_item = expected.pop_front();
    if (!actual.compare(exp_item)) // VIOLATION
      `uvm_fatal("SB", "mismatch")
  endfunction
endclass
