// Testcase: UVM-OBJ-003 good.sv
// Properly balanced objection control flow
`include "uvm_mock.svh"

class good_obj_test extends uvm_test;
  `uvm_component_utils(good_obj_test)

  function new(string name, uvm_component parent);
    super.new(name, parent);
  endfunction

  task run_phase(uvm_phase phase);
    int status = 0;
    phase.raise_objection(this);

    if (status != 0) begin
      `uvm_error("TEST", "Fatal configuration error encountered")
      // Do not return directly; fall through or drop objection first
    end else begin
      #10;
    end

    phase.drop_objection(this);
  endtask
endclass
