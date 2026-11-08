// SV-BUG-001 good: branches differ, or are trivial on purpose.
`include "uvm_mock.svh"

typedef enum {READ, WRITE, IDLE} kind_e;

class bus_driver extends uvm_component;
  `uvm_component_utils(bus_driver)
  int rd_cnt, wr_cnt;
  function new(string name, uvm_component parent);
    super.new(name, parent);
  endfunction

  function void count(kind_e kind);
    case (kind)
      READ:  rd_cnt++;
      WRITE: wr_cnt++;
      default: ;
    endcase
  endfunction

  function int classify(int v);
    if (v < 0) return 0;
    else if (v > 100) return 0;   // a lone return repeated on purpose
    else return 1;
  endfunction

  function void note(bit a);
    if (a) `uvm_info("DRV", "same message", UVM_LOW)
    else   `uvm_info("DRV", "same message", UVM_LOW)
  endfunction
endclass
