// UVM-TST-002 good: drain time set on the objection; waits on events before dropping.
`include "uvm_mock.svh"

class base_test extends uvm_test;
  `uvm_component_utils(base_test)
  event done;
  function new(string name, uvm_component parent);
    super.new(name, parent);
  endfunction

  task run_phase(uvm_phase phase);
    phase.raise_objection(this);
    phase.phase_done.set_drain_time(this, 1us);
    @(done);
    phase.drop_objection(this);
  endtask
endclass
