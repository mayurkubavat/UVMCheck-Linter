// Compliant Clean Test Case: SV-CON-012 (ForkLoopVariableCapture)
`include "uvm_mock.svh"

// Compliant placeholder - verified to produce 0 violations
class compliant_forkloopvariablecapture extends uvm_object;
    function new(string name = "compliant");
        super.new(name);
    endfunction
endclass

// The copy the rule asks for: a declaration of the fork itself is
// initialized by the parent thread, before the child threads start.
class fork_copy_bench;
  int ids[$];
  int i;
  task consume(int id);
    #1;
  endtask
  task spawn_all();
    for (int n = 0; n < 4; n++) begin
      fork
        automatic int k = n;
        consume(k);
      join_none
    end
    foreach (ids[j]) begin
      fork
        automatic int id = ids[j];
        begin
          consume(id);
        end
      join_none
    end
    for (i = 0; i < 4; i++) begin
      fork
        automatic int k = i;
        consume(k);
      join_none
    end
  endtask
endclass
