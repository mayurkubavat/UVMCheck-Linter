// SV-DUP-003: the constraint is declared once in a base; a subclass repeating it is SV-RND territory.
`include "uvm_mock.svh"

class bus_item_base extends uvm_sequence_item;
  `uvm_object_utils(bus_item_base)
  rand bit [31:0] addr;
  rand int unsigned len;
  constraint c_legal { addr[1:0] == 0; len inside {[1:16]}; addr < 32'h1000; }
  function new(string name = "bus_item_base");
    super.new(name);
  endfunction
endclass

class apb_item extends bus_item_base;
  `uvm_object_utils(apb_item)
  constraint c_legal { addr[1:0] == 0; len inside {[1:16]}; addr < 32'h1000; }
  function new(string name = "apb_item");
    super.new(name);
  endfunction
endclass

class ahb_item extends bus_item_base;
  `uvm_object_utils(ahb_item)
  constraint c_short { len < 4; }
  function new(string name = "ahb_item");
    super.new(name);
  endfunction
endclass

class spi_item extends uvm_sequence_item;
  `uvm_object_utils(spi_item)
  rand int unsigned len;
  constraint c_short { len < 4; }
  function new(string name = "spi_item");
    super.new(name);
  endfunction
endclass
