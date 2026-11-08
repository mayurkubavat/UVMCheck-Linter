// UVM-CPL-004: violations are marked.
`include "uvm_mock.svh"
class irq_monitor extends uvm_monitor;
  `uvm_component_utils(irq_monitor)
  function new(string name, uvm_component parent);
    super.new(name, parent);
  endfunction
  task run_phase(uvm_phase phase);
    @(posedge tb_top.irq); // VIOLATION
    if ($root.tb_top.u_dut.state == 2) // VIOLATION
      `uvm_info("IRQ", "busy", UVM_LOW)
  endtask
endclass

module dut;
  logic [1:0] state;
endmodule

module tb_top;
  logic irq;
  dut u_dut();
endmodule
