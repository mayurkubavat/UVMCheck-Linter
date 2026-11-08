// SV-ERR-003 good: $finish in the testbench top module is fine; classes use `uvm_fatal.
`include "uvm_mock.svh"

class err_counter extends uvm_component;
  `uvm_component_utils(err_counter)
  int err_cnt;
  function new(string name, uvm_component parent);
    super.new(name, parent);
  endfunction

  function void note_error();
    err_cnt++;
    if (err_cnt > 10) `uvm_fatal("SCB", "too many errors")
  endfunction
endclass

module tb_top;
  initial begin
    #1000;
    $finish;
  end
endmodule
