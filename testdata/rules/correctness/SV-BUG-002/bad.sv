// SV-BUG-002: the third condition repeats the first, so its branch is dead.
`include "uvm_mock.svh"

typedef enum {READ, WRITE, IDLE} kind_e;

class bus_monitor extends uvm_component;
  `uvm_component_utils(bus_monitor)
  int rd, wr;
  function new(string name, uvm_component parent);
    super.new(name, parent);
  endfunction

  function void sample(kind_e kind, int len);
    if (kind == READ) rd++;
    else if (kind == WRITE) wr++;
    else if (kind == READ && len > 0) rd += len;
    else if (kind == READ) rd--; // VIOLATION
  endfunction
endclass
