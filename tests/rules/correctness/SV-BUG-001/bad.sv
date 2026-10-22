// SV-BUG-001: a WRITE branch pasted from READ and never edited.
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
      READ:  begin rd_cnt++; `uvm_info("DRV", "read", UVM_HIGH) end
      WRITE: begin rd_cnt++; `uvm_info("DRV", "read", UVM_HIGH) end // VIOLATION
      default: ;
    endcase
  endfunction

  function int pick(bit sel, int a, int b);
    if (sel) begin
      a = a + b;
      return a;
    end else begin // VIOLATION
      a = a + b;
      return a;
    end
  endfunction
endclass
