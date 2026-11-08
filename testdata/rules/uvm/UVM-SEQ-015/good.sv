// The test creates the item.
`include "uvm_mock.svh"

class kept_item extends uvm_sequence_item;
  `uvm_object_utils(kept_item)
  function new(string name = "kept_item");
    super.new(name);
  endfunction : new
endclass : kept_item

class quiet_test extends uvm_test;
  `uvm_component_utils(quiet_test)
  function new(string name, uvm_component parent);
    super.new(name, parent);
    void'(kept_item::type_id::create("item"));
  endfunction : new
endclass : quiet_test
