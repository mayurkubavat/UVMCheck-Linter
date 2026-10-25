// UVM-CPL-004: compliant code.
`include "uvm_mock.svh"
interface irq_if;
  logic irq;
endinterface

class irq_monitor extends uvm_monitor;
  `uvm_component_utils(irq_monitor)
  virtual irq_if vif;
  function new(string name, uvm_component parent);
    super.new(name, parent);
  endfunction
  task run_phase(uvm_phase phase);
    @(posedge vif.irq);
  endtask
endclass
