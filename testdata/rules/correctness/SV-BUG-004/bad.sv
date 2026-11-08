// SV-BUG-004: do_copy and do_compare pair a field with the wrong field of rhs.
`include "uvm_mock.svh"

class bus_item extends uvm_sequence_item;
  `uvm_object_utils(bus_item)
  rand bit [31:0] addr;
  rand bit [31:0] data;
  rand bit [3:0]  be;
  function new(string name = "bus_item");
    super.new(name);
  endfunction

  virtual function void do_copy(uvm_object rhs);
    bus_item rhs_;
    if (!$cast(rhs_, rhs)) `uvm_fatal("CAST", "bad type")
    addr = rhs_.addr;
    data = rhs_.addr; // VIOLATION
    be   = rhs_.be;
  endfunction

  virtual function bit do_compare(uvm_object rhs, uvm_comparer comparer);
    bus_item rhs_;
    if (!$cast(rhs_, rhs)) return 0;
    return addr == rhs_.addr && data == rhs_.be; // VIOLATION
  endfunction
endclass
