// SV-MET-001: deeply nested concurrent driver code.
`include "uvm_mock.svh"

class bus_driver extends uvm_component;
  `uvm_component_utils(bus_driver)
  bit ready, busy, err, stall;
  int cnt, mode;
  function new(string name, uvm_component parent);
    super.new(name, parent);
  endfunction

  task drive(int n); // VIOLATION
    forever begin
      fork
        begin
          for (int i = 0; i < n; i++) begin
            if (ready && !busy) begin
              case (mode)
                0: if (err || stall) cnt++; else cnt--;
                1: while (busy) begin if (err) break; cnt++; end
                default: ;
              endcase
            end else if (stall) begin
              repeat (2) if (err && ready || busy) cnt = cnt > 0 ? cnt - 1 : 0;
            end else begin
              foreach (cnt[j]) if (busy) cnt++;
            end
          end
        end
      join_none
    end
  endtask
endclass
