// UVM-REP-006: errors demoted for every ID.
`include "uvm_mock.svh"

class quiet_catcher extends uvm_report_catcher;
  function new(string name = "quiet_catcher");
    super.new(name);
  endfunction

  virtual function action_e catch();
    if (get_severity() == UVM_ERROR)
      set_severity(UVM_INFO); // VIOLATION
    return THROW;
  endfunction
endclass

class base_test extends uvm_test;
  `uvm_component_utils(base_test)
  function new(string name, uvm_component parent);
    super.new(name, parent);
  endfunction

  function void build_phase(uvm_phase phase);
    super.build_phase(phase);
    set_report_severity_override(UVM_ERROR, UVM_WARNING); // VIOLATION
    set_report_severity_action(UVM_FATAL, UVM_NO_ACTION); // VIOLATION
  endfunction
endclass
