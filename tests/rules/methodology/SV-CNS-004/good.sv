// SV-CNS-004: enums, short checks and argument dispatch (SV-CPL-001) are not flagged.
`include "uvm_mock.svh"
typedef enum {READ, WRITE, IDLE} mode_e;

// Stands in for a UVM API that hands back strings.
class uvm_fake_field;
  function string get_kind();
    return "a";
  endfunction
endclass

class bus_test extends uvm_test;
  `uvm_component_utils(bus_test)
  mode_e mode;
  string name_s;
  string policy, kind;
  uvm_fake_field fld;
  function new(string name, uvm_component parent);
    super.new(name, parent);
  endfunction
  function void apply(string op);
    if (op == "a") $display("a");
    else if (op == "b") $display("b");
    else if (op == "c") $display("c");
  endfunction
  task run_phase(uvm_phase phase);
    case (mode)
      READ: $display("r");
      WRITE: $display("w");
      IDLE: $display("i");
    endcase
    if (name_s == "") $display("unnamed");
    else if (name_s == "top") $display("top");
    if (name_s == "x") $display("x");
    case (policy)
      "RW": $display("rw");
      "RO": $display("ro");
      "W1C": $display("w1c");
    endcase
    kind = fld.get_kind();
    if (kind == "a") $display("a");
    else if (kind == "b") $display("b");
    else if (kind == "c") $display("c");
    if (name_s == "y") $display("y");
  endtask
endclass
