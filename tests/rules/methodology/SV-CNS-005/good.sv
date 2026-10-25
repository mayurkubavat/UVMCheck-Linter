// SV-CNS-005: packed structs, two slices and indexed part-selects are fine.
`include "uvm_mock.svh"
typedef struct packed { logic [7:0] flags; logic [7:0] kind; logic [15:0] len; } hdr_t;

class pkt_monitor extends uvm_monitor;
  `uvm_component_utils(pkt_monitor)
  hdr_t hdr;
  logic [31:0] raw;
  logic [63:0] data;
  int len, kind;
  function new(string name, uvm_component parent);
    super.new(name, parent);
  endfunction
  function void decode();
    len = hdr.len;
    kind = hdr[23:16];
    len = raw[15:0];
    kind = raw[31:16];
    for (int i = 0; i < 8; i++) len += data[i*8 +: 8];
  endfunction
endclass
