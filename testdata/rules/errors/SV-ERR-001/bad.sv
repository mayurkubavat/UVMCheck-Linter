// SV-ERR-001: failures detected, then only logged as info or ignored.
`include "uvm_mock.svh"

class bus_item extends uvm_sequence_item;
  `uvm_object_utils(bus_item)
  rand bit [31:0] addr;
  function new(string name = "bus_item");
    super.new(name);
  endfunction
endclass

class bus_seq extends uvm_sequence #(bus_item);
  `uvm_object_utils(bus_seq)
  function new(string name = "bus_seq");
    super.new(name);
  endfunction

  virtual task body();
    bus_item tr = new("tr");
    uvm_object obj;
    if (!tr.randomize()) // VIOLATION
      `uvm_info("RND", "randomize failed", UVM_LOW)
    if (!$cast(tr, obj)) begin // VIOLATION
    end
    assert (tr.randomize()) else `uvm_warning("RND", "randomize failed") // VIOLATION
  endtask
endclass
