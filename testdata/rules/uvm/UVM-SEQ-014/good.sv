// The test creates the sequence.
`include "uvm_mock.svh"

class kept_seq extends uvm_sequence;
  `uvm_object_utils(kept_seq)
  function new(string name = "kept_seq");
    super.new(name);
  endfunction : new
  task body();
    int n;
    n = n + 1;
  endtask : body
endclass : kept_seq

class quiet_test extends uvm_test;
  `uvm_component_utils(quiet_test)
  function new(string name, uvm_component parent);
    super.new(name, parent);
    void'(kept_seq::type_id::create("seq"));
  endfunction : new
endclass : quiet_test
