// SV-DED-013 good: used arguments, virtual hooks, phases and marked-unused names.
`include "uvm_mock.svh"

class clk_utils;
  int unsigned cycles;
  task wait_clks(int n);
    repeat (n) cycles++;
  endtask
  virtual function void on_item(int id);
    cycles++;
  endfunction
  function void tick(int unused_id);
    cycles++;
  endfunction
endclass

class bus_monitor extends uvm_monitor;
  int unsigned built;
  function new(string name, uvm_component parent);
    super.new(name, parent);
  endfunction
  function void build_phase(uvm_phase phase);
    built++;
  endfunction
endclass
