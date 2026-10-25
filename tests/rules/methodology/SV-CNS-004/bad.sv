// SV-CNS-004: violations are marked.
`include "uvm_mock.svh"
class bus_test extends uvm_test;
  `uvm_component_utils(bus_test)
  string mode;
  function new(string name, uvm_component parent);
    super.new(name, parent);
  endfunction
  task run_phase(uvm_phase phase);
    string speed;
    case (mode) // VIOLATION
      "read": $display("r");
      "write": $display("w");
      "idle", "sleep": $display("i");
    endcase
    void'($value$plusargs("SPEED=%s", speed));
    if (speed == "slow") // VIOLATION
      #100;
    else if (speed == "medium")
      #50;
    else if (speed == "fast" || speed == "turbo")
      #10;
  endtask
endclass
