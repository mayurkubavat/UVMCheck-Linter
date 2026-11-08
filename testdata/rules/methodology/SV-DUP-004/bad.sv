// SV-DUP-004: a report ID written in many places.
`include "uvm_mock.svh"

class bus_scoreboard extends uvm_component;
  `uvm_component_utils(bus_scoreboard)
  int errors;
  function new(string name, uvm_component parent);
    super.new(name, parent);
  endfunction

  function void check_addr(int a, int b);
    if (a != b) `uvm_error("BUS_SCBD", "address mismatch")
  endfunction

  function void check_data(int a, int b);
    if (a != b) `uvm_error("BUS_SCBD", "data mismatch")
  endfunction

  function void check_len(int a, int b);
    if (a != b) `uvm_error("BUS_SCBD", "length mismatch")
  endfunction

  function void report_phase(uvm_phase phase);
    `uvm_info("BUS_SCBD", $sformatf("%0d errors", errors), UVM_LOW)
  endfunction
endclass
