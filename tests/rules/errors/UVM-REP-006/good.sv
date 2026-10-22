// UVM-REP-006 good: demotion limited to one known ID.
`include "uvm_mock.svh"

class known_issue_catcher extends uvm_report_catcher;
  function new(string name = "known_issue_catcher");
    super.new(name);
  endfunction

  virtual function action_e catch();
    if (get_severity() == UVM_ERROR && get_id() == "AXI_RESP_SLVERR")
      set_severity(UVM_WARNING);
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
    set_report_severity_action(UVM_INFO, UVM_NO_ACTION);
    set_report_severity_action(UVM_ERROR, UVM_DISPLAY | UVM_COUNT);
  endfunction
endclass
