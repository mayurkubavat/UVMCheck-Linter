// SV-BUG-003 good: this. qualifies the property; output args are not self-assignments.
`include "uvm_mock.svh"

class bus_cfg extends uvm_object;
  `uvm_object_utils(bus_cfg)
  int unsigned depth;
  function new(string name = "bus_cfg", int unsigned depth = 8);
    super.new(name);
    this.depth = depth;
  endfunction

  function void get_depth(output int unsigned d);
    d = depth;
  endfunction
endclass
