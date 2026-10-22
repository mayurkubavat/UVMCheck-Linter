// SV-DSN-006 good: constructors, overrides that do more, and different arguments.
`include "uvm_mock.svh"

class base_scale;
  virtual function int scale(int v);
    return v * 2;
  endfunction
endclass

class half_scale extends base_scale;
  virtual function int scale(int v);
    return super.scale(v / 2);
  endfunction
endclass

class bus_agent extends uvm_agent;
  int unsigned built;
  function new(string name, uvm_component parent);
    super.new(name, parent);
  endfunction
  function void build_phase(uvm_phase phase);
    super.build_phase(phase);
    built++;
  endfunction
endclass
