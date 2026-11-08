// UVM-ROL-005: violations are marked.
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
  bus_driver drv; // VIOLATION
  uvm_sequencer #(bus_item) sqr; // VIOLATION
  function new(string name, uvm_component parent);
    super.new(name, parent);
  endfunction
endclass
