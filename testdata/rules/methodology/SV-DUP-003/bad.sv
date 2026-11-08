// SV-DUP-003: one constraint pasted into two unrelated item classes.
`include "uvm_mock.svh"

class apb_item extends uvm_sequence_item;
  `uvm_object_utils(apb_item)
  rand bit [31:0] addr;
  rand int unsigned len;
  constraint c_legal { addr[1:0] == 0; len inside {[1:16]}; addr < 32'h1000; }
  function new(string name = "apb_item");
    super.new(name);
  endfunction
endclass

class ahb_item extends uvm_sequence_item;
  `uvm_object_utils(ahb_item)
  rand bit [31:0] addr;
  rand int unsigned len;
  constraint c_addr { addr[1:0] == 0; len inside {[1:16]}; addr < 32'h1000; }
  function new(string name = "ahb_item");
    super.new(name);
  endfunction
endclass
