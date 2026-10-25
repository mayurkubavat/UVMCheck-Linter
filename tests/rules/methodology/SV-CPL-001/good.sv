// SV-CPL-001: enum arguments, single checks and overrides are fine.
`include "uvm_mock.svh"
typedef enum {OP_READ, OP_WRITE} op_e;

class base_seq extends uvm_sequence;
  `uvm_object_utils(base_seq)
  function new(string name = "base_seq");
    super.new(name);
  endfunction
  virtual function void set_mode(string m);
  endfunction
endclass

class bus_seq extends base_seq;
  `uvm_object_utils(bus_seq)
  function new(string name = "bus_seq");
    super.new(name);
  endfunction
  function void do_op(op_e kind);
    if (kind == OP_READ) $display("r");
  endfunction
  function void log(string tag);
    if (tag == "") tag = "BUS";
    $display("%s", tag);
  endfunction
  function void set_mode(string m);
    if (m == "a") $display("a");
    else if (m == "b") $display("b");
  endfunction
endclass
