// SV-TST-001 good: the test may pin a seed; seeds from variables are fine.
`include "uvm_mock.svh"

class bus_seq extends uvm_sequence;
  rand int unsigned len;
  int unsigned seed;
  function new(string name = "bus_seq");
    super.new(name);
    this.srandom(seed);
  endfunction
endclass

class repro_test extends uvm_test;
  function new(string name, uvm_component parent);
    super.new(name, parent);
    this.srandom(7);
  endfunction
endclass
