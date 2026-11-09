// Test Case: UVM-SEQ-004 (SequenceAbortSafety)
`include "uvm_mock.svh"

class abort_item extends uvm_sequence_item;
  rand bit [7:0] data;
  function new(string name = "abort_item");
    super.new(name);
  endfunction
endclass

class abort_seq extends uvm_sequence #(abort_item);
  bit stop;
  function new(string name = "abort_seq");
    super.new(name);
  endfunction
  // VIOLATION UVM-SEQ-004: the return leaves start_item() without finish_item().
  task body();
    abort_item it = new("it");
    start_item(it);
    if (stop) return;
    finish_item(it);
  endtask
endclass
