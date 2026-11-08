// SV-BUG-004 good: every field paired with its namesake; other methods are not checked.
`include "uvm_mock.svh"

class bus_item extends uvm_sequence_item;
  `uvm_object_utils(bus_item)
  rand bit [31:0] addr;
  rand bit [31:0] data;
  function new(string name = "bus_item");
    super.new(name);
  endfunction

  virtual function void do_copy(uvm_object rhs);
    bus_item rhs_;
    if (!$cast(rhs_, rhs)) `uvm_fatal("CAST", "bad type")
    addr = rhs_.addr;
    this.data = rhs_.data;
  endfunction

  virtual function bit do_compare(uvm_object rhs, uvm_comparer comparer);
    bus_item rhs_;
    if (!$cast(rhs_, rhs)) return 0;
    return addr == rhs_.addr && data == rhs_.data;
  endfunction

  function void swap_halves(bus_item other);
    addr = other.data; // not a copy/compare method
  endfunction
endclass
