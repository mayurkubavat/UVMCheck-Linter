// UVM-TST-002: fixed delays used to let traffic drain before the objection drops.
`include "uvm_mock.svh"

class base_test extends uvm_test;
  `uvm_component_utils(base_test)
  function new(string name, uvm_component parent);
    super.new(name, parent);
  endfunction

  task run_phase(uvm_phase phase);
    phase.raise_objection(this);
    #500;
    #1us; // VIOLATION
    phase.drop_objection(this);
  endtask

  task main_phase(uvm_phase phase);
    phase.raise_objection(this);
    #100 phase.drop_objection(this); // VIOLATION
  endtask
endclass
