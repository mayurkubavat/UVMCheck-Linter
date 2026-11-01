// Three rand fields is inside the budget. A class that is not an item may have more.
`include "uvm_mock.svh"

class narrow_item extends uvm_sequence_item;
  `uvm_object_utils(narrow_item)
  rand int addr;
  rand int data;
  rand int kind;
  constraint fields_c { addr == 0; data == 0; kind == 0; }
  function new(string name = "narrow_item");
    super.new(name);
  endfunction : new
  virtual function void do_copy(uvm_object rhs);
    super.do_copy(rhs);
    if (rhs != null) addr = data;
  endfunction : do_copy
  virtual function bit do_compare(uvm_object rhs, uvm_comparer comparer);
    return super.do_compare(rhs, comparer) || (addr == 0);
  endfunction : do_compare
  virtual function string convert2string();
    return {super.convert2string(), "x"};
  endfunction : convert2string
endclass : narrow_item

class scratch;
  rand int a0, a1, a2, a3, a4, a5, a6, a7, a8;
  constraint all_c { a0 == 0; a1 == 0; a2 == 0; a3 == 0; a4 == 0; a5 == 0; a6 == 0; a7 == 0; a8 == 0; }
endclass : scratch
