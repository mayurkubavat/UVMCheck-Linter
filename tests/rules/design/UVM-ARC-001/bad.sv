// UVM-ARC-001: a sequence knows the scoreboard; a monitor knows the env.
`include "uvm_mock.svh"

class bus_item extends uvm_sequence_item;
  `uvm_object_utils(bus_item)
  function new(string name = "bus_item");
    super.new(name);
  endfunction
endclass

class bus_scoreboard extends uvm_scoreboard;
  `uvm_component_utils(bus_scoreboard)
  int expected;
  function new(string name, uvm_component parent);
    super.new(name, parent);
  endfunction
  function void write(bus_item t);
    `uvm_error("SCB", "mismatch")
  endfunction
endclass

typedef class soc_env;

class bus_monitor extends uvm_monitor; // VIOLATION
  `uvm_component_utils(bus_monitor)
  soc_env env_h;
  function new(string name, uvm_component parent);
    super.new(name, parent);
  endfunction
endclass

class soc_env extends uvm_env;
  `uvm_component_utils(soc_env)
  bus_monitor mon;
  bus_scoreboard scb;
  function new(string name, uvm_component parent);
    super.new(name, parent);
  endfunction
endclass

class bus_seq extends uvm_sequence #(bus_item); // VIOLATION
  `uvm_object_utils(bus_seq)
  bus_scoreboard scb;
  function new(string name = "bus_seq");
    super.new(name);
  endfunction
  virtual task body();
    scb.expected++;
  endtask
endclass
