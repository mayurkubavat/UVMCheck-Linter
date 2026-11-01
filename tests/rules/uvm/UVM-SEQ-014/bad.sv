// The test never names the sequence.
`include "uvm_mock.svh"

class orphan_seq extends uvm_sequence;
  `uvm_object_utils(orphan_seq)
  function new(string name = "orphan_seq");
    super.new(name);
  endfunction : new
  task body();
    int n;
    n = n + 1;
  endtask : body
endclass : orphan_seq

class quiet_test extends uvm_test;
  `uvm_component_utils(quiet_test)
  function new(string name, uvm_component parent);
    super.new(name, parent);
  endfunction : new
endclass : quiet_test
