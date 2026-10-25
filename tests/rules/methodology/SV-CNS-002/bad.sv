// SV-CNS-002: violations are marked.
`include "uvm_mock.svh"
`define CLEAR_COUNTERS(c) c.rd = 0; c.wr = 0; // VIOLATION
`define WAIT_CYCLES(vif, n) begin repeat (n) @(posedge vif.clk); end // VIOLATION
`define CHECK_RANGE(v, lo, hi) /* VIOLATION */ \
  if ((v) < (lo)) $error("low"); \
  if ((v) > (hi)) $error("high");

class counters;
  int rd, wr;
endclass

class bus_driver extends uvm_driver;
  `uvm_component_utils(bus_driver)
  counters cnt = new();
  virtual bus_if vif;
  function new(string name, uvm_component parent);
    super.new(name, parent);
  endfunction
  task run_phase(uvm_phase phase);
    `CLEAR_COUNTERS(cnt)
    `WAIT_CYCLES(vif, 4)
    `CHECK_RANGE(cnt.rd, 0, 10)
  endtask
endclass
