// UVM-ARC-001 good: dependencies point down; config objects are shared by all layers.
`include "uvm_mock.svh"

class bus_cfg extends uvm_object;
  `uvm_object_utils(bus_cfg)
  function new(string name = "bus_cfg");
    super.new(name);
  endfunction
endclass

class bus_item extends uvm_sequence_item;
  `uvm_object_utils(bus_item)
  function new(string name = "bus_item");
    super.new(name);
  endfunction
endclass

class bus_sequencer extends uvm_sequencer #(bus_item);
  `uvm_component_utils(bus_sequencer)
  function new(string name, uvm_component parent);
    super.new(name, parent);
  endfunction
endclass

class bus_seq extends uvm_sequence #(bus_item);
  `uvm_object_utils(bus_seq)
  bus_cfg cfg;
  bus_sequencer p_sqr;
  function new(string name = "bus_seq");
    super.new(name);
  endfunction
endclass

class bus_monitor extends uvm_monitor;
  `uvm_component_utils(bus_monitor)
  bus_cfg cfg;
  bus_item last;
  function new(string name, uvm_component parent);
    super.new(name, parent);
  endfunction
endclass

class soc_env extends uvm_env;
  `uvm_component_utils(soc_env)
  bus_monitor mon;
  bus_sequencer sqr;
  function new(string name, uvm_component parent);
    super.new(name, parent);
  endfunction
endclass

class base_test extends uvm_test;
  `uvm_component_utils(base_test)
  soc_env env;
  function new(string name, uvm_component parent);
    super.new(name, parent);
  endfunction
endclass
