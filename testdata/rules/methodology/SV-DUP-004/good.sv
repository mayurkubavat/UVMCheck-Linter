// SV-DUP-004: the ID is a named constant; short and format-only strings are fine.
`include "uvm_mock.svh"

class bus_scoreboard extends uvm_component;
  `uvm_component_utils(bus_scoreboard)
  localparam string ID = "BUS_SCBD";
  int errors;
  bus_scoreboard peer;
  function new(string name, uvm_component parent);
    super.new(name, parent);
  endfunction

  function void check_addr(int a, int b);
    if (a != b) `uvm_error(ID, $sformatf("%0d", a))
  endfunction

  function void check_data(int a, int b);
    if (a != b) `uvm_error(ID, $sformatf("%0d", a))
  endfunction

  function void check_len(int a, int b);
    if (a != b) `uvm_error(ID, $sformatf("%0d", a))
  endfunction

  function void report_phase(uvm_phase phase);
    `uvm_info(ID, $sformatf("%0d", errors), UVM_LOW)
    `uvm_info("OK", "ok", UVM_LOW)
    `uvm_info("OK", "ok", UVM_LOW)
    `uvm_info("OK", "ok", UVM_LOW)
    `uvm_info("OK", "ok", UVM_LOW)
  endfunction

  function void build_phase(uvm_phase phase);
    uvm_config_db#(int)::set(this, "*", "bus_timeout", 10);
    uvm_config_db#(int)::set(this, "a", "bus_timeout", 10);
    uvm_config_db#(int)::set(this, "b", "bus_timeout", 10);
    uvm_config_db#(int)::set(this, "c", "bus_timeout", 10);
  endfunction
endclass
