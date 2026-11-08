// UVM-TST-001 good: checks via a helper, via a base class, or via an assertion.
`include "uvm_mock.svh"

class bus_item extends uvm_sequence_item;
  `uvm_object_utils(bus_item)
  function new(string name = "bus_item");
    super.new(name);
  endfunction
endclass

class base_scoreboard extends uvm_scoreboard;
  `uvm_component_utils(base_scoreboard)
  function new(string name, uvm_component parent);
    super.new(name, parent);
  endfunction
  function void mismatch(string what);
    `uvm_error("SCB", what)
  endfunction
endclass

class bus_scoreboard extends base_scoreboard;
  `uvm_component_utils(bus_scoreboard)
  bus_item exp_q[$];
  function new(string name, uvm_component parent);
    super.new(name, parent);
  endfunction
  function void write(bus_item t);
    if (exp_q.size() == 0) mismatch("unexpected item");
  endfunction
endclass

class len_checker extends uvm_subscriber #(bus_item);
  `uvm_component_utils(len_checker)
  function new(string name, uvm_component parent);
    super.new(name, parent);
  endfunction
  function void write(bus_item t);
    assert (t != null);
  endfunction
endclass
