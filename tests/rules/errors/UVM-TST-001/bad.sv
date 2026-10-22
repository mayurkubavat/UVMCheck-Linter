// UVM-TST-001: the scoreboard collects items but never compares or reports.
`include "uvm_mock.svh"

class bus_item extends uvm_sequence_item;
  `uvm_object_utils(bus_item)
  function new(string name = "bus_item");
    super.new(name);
  endfunction
endclass

class bus_scoreboard extends uvm_scoreboard; // VIOLATION
  `uvm_component_utils(bus_scoreboard)
  bus_item exp_q[$];
  function new(string name, uvm_component parent);
    super.new(name, parent);
  endfunction

  function void write(bus_item t);
    exp_q.push_back(t);
    `uvm_info("SCB", "got item", UVM_HIGH)
  endfunction
endclass
