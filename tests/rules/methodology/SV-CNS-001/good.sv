// SV-CNS-001: typed constants, switches and code macros are fine.
`include "uvm_mock.svh"
`define ENABLE_TRACE
`define BUS_MSB(w) ((w) - 1)
`define DATA_WIDTH bus_pkg::DATA_WIDTH
`define LOG_MSG(m) $display("%s", m)
`define FILL_VALUE '0
`ifndef BUILD_SEED
  `define BUILD_SEED 1
`endif

package bus_pkg;
  localparam int DATA_WIDTH = 32;
  localparam string LOG_FILE = "bus_trace.log";
endpackage

class bus_cfg extends uvm_object;
  `uvm_object_utils(bus_cfg)
  logic [`BUS_MSB(`DATA_WIDTH):0] mask;
  string log_file = bus_pkg::LOG_FILE;
  function new(string name = "bus_cfg");
    super.new(name);
    `LOG_MSG(log_file);
  endfunction
endclass
