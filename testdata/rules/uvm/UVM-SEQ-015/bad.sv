// The test never names the item.
`include "uvm_mock.svh"

class orphan_item extends uvm_sequence_item;
  `uvm_object_utils(orphan_item)
  function new(string name = "orphan_item");
    super.new(name);
  endfunction : new
endclass : orphan_item

class quiet_test extends uvm_test;
  `uvm_component_utils(quiet_test)
  function new(string name, uvm_component parent);
    super.new(name, parent);
  endfunction : new
endclass : quiet_test
