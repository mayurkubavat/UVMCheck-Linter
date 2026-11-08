// SV-ERR-003: $finish from inside a component skips check/report phases.
`include "uvm_mock.svh"

class err_counter extends uvm_component;
  `uvm_component_utils(err_counter)
  int err_cnt;
  function new(string name, uvm_component parent);
    super.new(name, parent);
  endfunction

  function void note_error();
    err_cnt++;
    if (err_cnt > 10) $finish; // VIOLATION
  endfunction
endclass
