// SV-BUG-002 good: distinct conditions, and random draws that may differ each time.
`include "uvm_mock.svh"

typedef enum {READ, WRITE, IDLE} kind_e;

class bus_monitor extends uvm_component;
  `uvm_component_utils(bus_monitor)
  int rd, wr;
  function new(string name, uvm_component parent);
    super.new(name, parent);
  endfunction

  function void sample(kind_e kind);
    if (kind == READ) rd++;
    else if (kind == WRITE) wr++;
    else if (kind == IDLE) ;
  endfunction

  function int pick();
    if ($urandom_range(1, 0) == 1) return 1;
    else if ($urandom_range(1, 0) == 1) return 2;
    return 3;
  endfunction
endclass
