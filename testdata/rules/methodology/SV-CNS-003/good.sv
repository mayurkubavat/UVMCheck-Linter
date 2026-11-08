// SV-CNS-003: include guards, simulator shims and switches outside classes are fine.
`ifndef BUS_MONITOR_SVH
`define BUS_MONITOR_SVH
`include "uvm_mock.svh"

`ifdef BUS_COVERAGE
`define HAS_COV 1
`endif

class bus_cfg extends uvm_object;
  `uvm_object_utils(bus_cfg)
  bit has_coverage;
  function new(string name = "bus_cfg");
    super.new(name);
  endfunction
endclass

class bus_monitor extends uvm_monitor;
  `uvm_component_utils(bus_monitor)
  bus_cfg cfg;
`ifdef UVM_VERSION_1_1
  int legacy;
`endif
  function new(string name, uvm_component parent);
    super.new(name, parent);
  endfunction
  task run_phase(uvm_phase phase);
`ifdef VCS
    $display("vcs");
`elsif QUESTA
    $display("questa");
`endif
    if (cfg.has_coverage) $display("cov");
  endtask
endclass
`endif
