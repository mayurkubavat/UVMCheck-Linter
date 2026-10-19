// Testcase: UVM-OBJ-003 bad.sv
// Early return while phase objection is held
`include "uvm_mock.svh"

class bad_obj_test extends uvm_test;
  `uvm_component_utils(bad_obj_test)

  function new(string name, uvm_component parent);
    super.new(name, parent);
  endfunction

  task run_phase(uvm_phase phase);
    int status = -1;
    phase.raise_objection(this);

    if (status != 0) begin
      `uvm_error("TEST", "Fatal configuration error encountered")
      return; // VIOLATION: Early return leaks objection and causes simulation hang
    end

    #10;
    phase.drop_objection(this);
  endtask
endclass
