// Compliant Clean Test Case: SV-INT-001 (BlockingAssignmentToInterface)
`include "uvm_mock.svh"

// Compliant placeholder - verified to produce 0 violations
class compliant_blockingassignmenttointerface extends uvm_object;
    function new(string name = "compliant");
        super.new(name);
    endfunction
endclass

// A continuous assignment to an interface signal is not a procedural drive:
// there is no race to avoid, and 'assign x <= y' would not compile.
interface blk_if;
    logic r;
endinterface : blk_if

module blk_top;
    blk_if vif();
    logic rst_n;
    assign vif.r = ~rst_n;
endmodule

// Configuration bits of an interface written in build_phase, before
// simulation time starts: nothing races with them.
interface cfg_bus_if;
  bit is_push_agent;
  logic valid;
endinterface

class cfg_bus_agent extends uvm_component;
  virtual cfg_bus_if vif;
  bit push;
  function new(string name, uvm_component parent);
    super.new(name, parent);
  endfunction
  function void build_phase(uvm_phase phase);
    vif.is_push_agent = push;
  endfunction
endclass

// A handle that happens to be called 'cb' (a callback object, as in UVM's
// own register layer) is not a clocking block: a class cannot declare one.
class note_cb;
  string fname;
  int lineno;
endclass

class note_user;
  note_cb cbs[$];
  extern task mark(string fname, int lineno);
  task clear();
    foreach (cbs[i]) begin
      note_cb cb = cbs[i];
      cb.fname = "";
      cb.lineno = 0;
    end
  endtask
endclass

task note_user::mark(string fname, int lineno);
  foreach (cbs[i]) begin
    note_cb cb = cbs[i];
    cb.fname = fname;
    cb.lineno = lineno;
  end
endtask
