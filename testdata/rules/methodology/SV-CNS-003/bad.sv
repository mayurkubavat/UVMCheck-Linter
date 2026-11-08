// SV-CNS-003: violations are marked.
`include "uvm_mock.svh"
class bus_monitor extends uvm_monitor;
  `uvm_component_utils(bus_monitor)
`ifdef BUS_COVERAGE // VIOLATION
  int hits;
`endif
  function new(string name, uvm_component parent);
    super.new(name, parent);
  endfunction
  task run_phase(uvm_phase phase);
`ifndef FAST_MODE // VIOLATION
    #10;
`elsif SLOW_BUS // VIOLATION
    #20;
`endif
  endtask
endclass
