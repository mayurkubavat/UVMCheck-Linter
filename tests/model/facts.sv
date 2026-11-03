// Fixture for the model's C ABI test (tests/model/capi_smoke.c): one class
// that carries each fact the ABI reports. Line numbers are checked there.
`include "uvm_mock.svh"

interface facts_if;
  logic clk;
endinterface

class facts_item extends uvm_sequence_item;
  rand int addr;
  constraint addr_c { addr < 16; }
  function new(string name = "facts_item");
    super.new(name);
  endfunction
  extern virtual function int sum(ref int q[$], input int scale);
  extern function void probe();
endclass

function int facts_item::sum(ref int q[$], input int scale);
  int total;
  foreach (q[i]) total += q[i] * scale;
  return total;
endfunction

function void facts_item::probe();
  virtual facts_if vif;
endfunction
