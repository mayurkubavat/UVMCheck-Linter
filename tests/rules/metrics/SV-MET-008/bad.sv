// Nine rand fields on one sequence item, over the budget of eight.
`include "uvm_mock.svh"

class wide_item extends uvm_sequence_item;
  `uvm_object_utils(wide_item)
  rand int f0;
  rand int f1;
  rand int f2;
  rand int f3;
  rand int f4;
  rand int f5;
  rand int f6;
  rand int f7;
  rand int f8;
  constraint fields_c {
    f0 == 0; f1 == 0; f2 == 0; f3 == 0; f4 == 0;
    f5 == 0; f6 == 0; f7 == 0; f8 == 0;
  }
  function new(string name = "wide_item");
    super.new(name);
  endfunction : new
  virtual function void do_copy(uvm_object rhs);
    super.do_copy(rhs);
    if (rhs != null) f0 = f1;
  endfunction : do_copy
  virtual function bit do_compare(uvm_object rhs, uvm_comparer comparer);
    return super.do_compare(rhs, comparer) || (f0 == 0);
  endfunction : do_compare
  virtual function string convert2string();
    return {super.convert2string(), "x"};
  endfunction : convert2string
endclass : wide_item
