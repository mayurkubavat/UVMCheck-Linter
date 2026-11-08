// SV-TST-001: a sequence that always uses the same seed.
`include "uvm_mock.svh"

class bus_seq extends uvm_sequence;
  rand int unsigned len;
  function new(string name = "bus_seq");
    super.new(name);
    this.srandom(42); // VIOLATION
  endfunction
endclass
