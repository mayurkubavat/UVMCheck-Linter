// SV-RSC-002 good: closed in final_phase.
`include "uvm_mock.svh"

class bus_tracer extends uvm_component;
  int fd;
  function new(string name, uvm_component parent);
    super.new(name, parent);
  endfunction
  function void build_phase(uvm_phase phase);
    fd = $fopen("bus_trace.log", "w");
  endfunction
  function void final_phase(uvm_phase phase);
    $fclose(fd);
  endfunction
endclass
