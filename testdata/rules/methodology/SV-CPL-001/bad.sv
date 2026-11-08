// SV-CPL-001: violations are marked.
`include "uvm_mock.svh"
class bus_seq extends uvm_sequence;
  `uvm_object_utils(bus_seq)
  function new(string name = "bus_seq");
    super.new(name);
  endfunction
  function void do_op(string kind); // VIOLATION
    if (kind == "read") $display("r");
    else if (kind == "write") $display("w");
  endfunction
  task send(int n, string speed = "slow"); // VIOLATION
    case (speed)
      "slow": #10;
      "fast": #1;
    endcase
  endtask
endclass

function automatic int scale(string unit, int v); // VIOLATION
  return unit == "ns" ? v : (unit == "us" ? v * 1000 : v);
endfunction
