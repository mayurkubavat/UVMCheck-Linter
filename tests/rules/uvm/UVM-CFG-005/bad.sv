// Testcase: UVM-CFG-005 bad.sv
// Cross-component uvm_config_db type mismatch
`include "uvm_mock.svh"

class driver_c extends uvm_component;
  `uvm_component_utils(driver_c)
  string timeout_str;

  function new(string name, uvm_component parent);
    super.new(name, parent);
  endfunction

  function void build_phase(uvm_phase phase);
    super.build_phase(phase);
    // VIOLATION: Retrieves 'timeout_ns' as string, but test configured it as int
    void'(uvm_config_db#(string)::get(this, "", "timeout_ns", timeout_str));
  endfunction
endclass

class test_c extends uvm_component;
  `uvm_component_utils(test_c)
  driver_c drv;

  function new(string name, uvm_component parent);
    super.new(name, parent);
  endfunction

  function void build_phase(uvm_phase phase);
    int timeout_val = 5000;
    super.build_phase(phase);
    uvm_config_db#(int)::set(this, "*", "timeout_ns", timeout_val);
    drv = driver_c::type_id::create("drv", this);
  endfunction
endclass
