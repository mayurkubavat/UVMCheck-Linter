// UVM-ROL-002: violations are marked.
`include "uvm_mock.svh"
class bus_item extends uvm_sequence_item;
  `uvm_object_utils(bus_item)
  rand bit [3:0] kind;
  function new(string name = "bus_item");
    super.new(name);
  endfunction
endclass

class bus_driver extends uvm_driver #(bus_item);
  `uvm_component_utils(bus_driver)
  bit [3:0] kind;
  covergroup kind_cg; // VIOLATION
    coverpoint kind;
  endgroup
  function new(string name, uvm_component parent);
    super.new(name, parent);
    kind_cg = new();
  endfunction
endclass
