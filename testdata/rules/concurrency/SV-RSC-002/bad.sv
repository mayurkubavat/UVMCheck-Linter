// SV-RSC-002: a trace file that is never closed.
`include "uvm_mock.svh"

class bus_tracer extends uvm_component;
  int fd;
  function new(string name, uvm_component parent);
    super.new(name, parent);
  endfunction
  function void build_phase(uvm_phase phase);
    fd = $fopen("bus_trace.log", "w"); // VIOLATION
  endfunction
endclass
