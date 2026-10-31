`include "uvm_mock.svh"

class item_t extends uvm_sequence_item;
  `uvm_object_utils(item_t)
  function new(string name = "item_t"); super.new(name); endfunction
endclass

class fast_item_t extends item_t;
  `uvm_object_utils(fast_item_t)
  function new(string name = "fast_item_t"); super.new(name); endfunction
endclass

class py_test extends uvm_test;
  `uvm_component_utils(py_test)
  function new(string name, uvm_component parent); super.new(name, parent); endfunction
  function void build_phase(uvm_phase phase);
    item_t::type_id::set_type_override(fast_item_t::type_id::get());
  endfunction
endclass
