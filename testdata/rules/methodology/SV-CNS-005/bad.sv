// SV-CNS-005: violations are marked.
`include "uvm_mock.svh"
class pkt_monitor extends uvm_monitor;
  `uvm_component_utils(pkt_monitor)
  logic [31:0] hdr;
  int len, kind, flags;
  function new(string name, uvm_component parent);
    super.new(name, parent);
  endfunction
  function void decode();
    len = hdr[15:0]; // VIOLATION
    kind = hdr[23:16];
  endfunction
  function void check();
    flags = hdr[31:24];
    if (hdr[15:0] == 0) $display("empty");
  endfunction
endclass
