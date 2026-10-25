// SV-CNS-001: violations are marked.
`include "uvm_mock.svh"
`define BUS_WIDTH 32 // VIOLATION
`define MAX_BURST (16) // VIOLATION
`define RESET_VALUE 8'hFF // VIOLATION
`define TIMEOUT 100ns // VIOLATION
`define LOG_FILE "bus_trace.log" // VIOLATION
`define NEG_OFFSET -4 // VIOLATION

class bus_cfg extends uvm_object;
  `uvm_object_utils(bus_cfg)
  logic [`BUS_WIDTH-1:0] mask = `RESET_VALUE;
  int max_burst = `MAX_BURST + `NEG_OFFSET;
  string log_file = `LOG_FILE;
  function new(string name = "bus_cfg");
    super.new(name);
  endfunction
  task wait_timeout();
    #`TIMEOUT;
  endtask
endclass
