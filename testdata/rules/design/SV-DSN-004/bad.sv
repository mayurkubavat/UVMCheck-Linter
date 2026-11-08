// SV-DSN-004: a driver that knows about eleven other classes.
`include "uvm_mock.svh"
class dep0; int v; endclass
class dep1; int v; endclass
class dep2; int v; endclass
class dep3; int v; endclass
class dep4; int v; endclass
class dep5; int v; endclass
class dep6; int v; endclass
class dep7; int v; endclass
class dep8; int v; endclass
class dep9; int v; endclass
class dep10; int v; endclass

class bus_driver extends uvm_driver; // VIOLATION
  `uvm_component_utils(bus_driver)
  dep0 d0;
  dep1 d1;
  dep2 d2;
  dep3 d3;
  dep4 d4;
  dep5 d5;
  dep6 d6;
  dep7 d7;
  dep8 d8;
  dep9 d9;
  dep10 d10;
  function new(string name, uvm_component parent);
    super.new(name, parent);
  endfunction
endclass
