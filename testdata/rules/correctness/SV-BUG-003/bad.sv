// SV-BUG-003: the argument hides the property, so the property is never set.
`include "uvm_mock.svh"

class bus_cfg extends uvm_object;
  `uvm_object_utils(bus_cfg)
  int unsigned depth;
  int unsigned width;
  function new(string name = "bus_cfg", int unsigned depth = 8);
    super.new(name);
    depth = depth; // VIOLATION
  endfunction

  function void set_width(int unsigned w);
    width = width; // VIOLATION
  endfunction
endclass
