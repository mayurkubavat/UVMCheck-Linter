// SV-MET-001 good: the same work split into small tasks; report macros do not count.
`include "uvm_mock.svh"

class bus_driver extends uvm_component;
  `uvm_component_utils(bus_driver)
  bit ready, busy, err;
  int cnt, mode;
  function new(string name, uvm_component parent);
    super.new(name, parent);
  endfunction

  task drive(int n);
    forever begin
      for (int i = 0; i < n; i++) step();
      `uvm_info("DRV", "a", UVM_HIGH)
      `uvm_info("DRV", "b", UVM_HIGH)
      `uvm_info("DRV", "c", UVM_HIGH)
      `uvm_info("DRV", "d", UVM_HIGH)
      `uvm_info("DRV", "e", UVM_HIGH)
    end
  endtask

  function void step();
    if (!ready || busy) return;
    case (mode)
      0: cnt++;
      default: cnt--;
    endcase
  endfunction
endclass
