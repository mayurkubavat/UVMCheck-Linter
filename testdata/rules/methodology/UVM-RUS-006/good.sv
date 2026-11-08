// UVM-RUS-006: compliant code.
`include "uvm_mock.svh"
class bus_item extends uvm_sequence_item;
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
  function bit do_compare(uvm_object rhs, uvm_comparer comparer);
    bus_item rhs_;
    if (!$cast(rhs_, rhs)) return 0;
    return super.do_compare(rhs, comparer) && addr == rhs_.addr && data == rhs_.data;
  endfunction
  function string convert2string();
    return $sformatf("addr=%0h data=%0h", addr, data);
  endfunction
endclass

// A derived item inherits the contract from its user base class.
class burst_item extends bus_item;
  `uvm_object_utils(burst_item)
  rand int unsigned len;
  function new(string name = "burst_item");
    super.new(name);
  endfunction
endclass

// Field macros provide the contract.
class reg_item extends uvm_sequence_item;
  rand bit [7:0] value;
  `uvm_object_utils_begin(reg_item)
    `uvm_field_int(value, UVM_ALL_ON)
  `uvm_object_utils_end
  function new(string name = "reg_item");
    super.new(name);
  endfunction
endclass
