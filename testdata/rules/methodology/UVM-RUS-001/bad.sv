// UVM-RUS-001: violations are marked.
`include "uvm_mock.svh"
class bus_item extends uvm_sequence_item;
  `uvm_object_utils(bus_item)
  function new(string name = "bus_item");
    super.new(name);
  endfunction
endclass

class bus_driver extends uvm_driver #(bus_item);
  `uvm_component_utils(bus_driver)
  function new(string name, uvm_component parent);
    super.new(name, parent);
  endfunction
endclass

class bus_monitor extends uvm_monitor;
  `uvm_component_utils(bus_monitor)
  function new(string name, uvm_component parent);
    super.new(name, parent);
  endfunction
endclass

typedef uvm_sequencer #(bus_item) bus_sequencer;
class bus_agent extends uvm_agent;
  `uvm_component_utils(bus_agent)
  bus_driver drv;
  bus_sequencer sqr;
  bus_monitor mon;
  function new(string name, uvm_component parent);
    super.new(name, parent);
  endfunction
  function void build_phase(uvm_phase phase);
    super.build_phase(phase);
    mon = bus_monitor::type_id::create("mon", this);
    drv = bus_driver::type_id::create("drv", this); // VIOLATION
    sqr = bus_sequencer::type_id::create("sqr", this); // VIOLATION
  endfunction
endclass
