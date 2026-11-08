// UVM-CFG-006: the driver reads plusargs directly.
`include "uvm_mock.svh"

class bus_driver extends uvm_driver;
  `uvm_component_utils(bus_driver)
  int unsigned delay;
  bit slow;
  function new(string name, uvm_component parent);
    super.new(name, parent);
  endfunction
  function void build_phase(uvm_phase phase);
    if (!$value$plusargs("BUS_DELAY=%d", delay)) delay = 0; // VIOLATION
    slow = $test$plusargs("SLOW_BUS"); // VIOLATION
  endfunction
endclass
