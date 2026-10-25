// UVM-RUS-006: violations are marked.
`include "uvm_mock.svh"
class bus_item extends uvm_sequence_item; // VIOLATION
  `uvm_object_utils(bus_item)
  rand bit [31:0] addr;
  rand bit [31:0] data;
  function new(string name = "bus_item");
    super.new(name);
  endfunction
  function void do_copy(uvm_object rhs);
    bus_item rhs_;
    super.do_copy(rhs);
    if ($cast(rhs_, rhs)) begin
      addr = rhs_.addr;
      data = rhs_.data;
    end
  endfunction
endclass
