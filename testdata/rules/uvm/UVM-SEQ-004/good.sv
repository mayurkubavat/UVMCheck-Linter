// Compliant Clean Test Case: UVM-SEQ-004 (SequenceAbortSafety)
`include "uvm_mock.svh"

class steady_item extends uvm_sequence_item;
  rand bit [7:0] data;
  function new(string name = "steady_item");
    super.new(name);
  endfunction
endclass

class steady_seq extends uvm_sequence #(steady_item);
  bit stop;
  function new(string name = "steady_seq");
    super.new(name);
  endfunction
  task body();
    steady_item it = new("it");
    if (stop) return;
    start_item(it);
    finish_item(it);
  endtask
endclass

// The handshake's own definition is not a call of it (uvm_sequence_base).
class own_handshake_seq extends uvm_sequence #(steady_item);
  function new(string name = "own_handshake_seq");
    super.new(name);
  endfunction
  virtual task start_item(uvm_sequence_item item, int set_priority = -1, uvm_sequencer_base sequencer = null);
    if (item == null) return;
    super.start_item(item, set_priority, sequencer);
  endtask
endclass
