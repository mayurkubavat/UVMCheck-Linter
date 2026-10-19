// Testcase: UVM-CFG-005 good.sv
// Properly matched cross-component uvm_config_db types
`include "uvm_mock.svh"

class good_driver_c extends uvm_component;
  `uvm_component_utils(good_driver_c)
  int timeout_val;

  function new(string name, uvm_component parent);
    super.new(name, parent);
  endfunction

  function void build_phase(uvm_phase phase);
    super.build_phase(phase);
    void'(uvm_config_db#(int)::get(this, "", "timeout_ns", timeout_val));
  endfunction
endclass

class good_test_c extends uvm_component;
  `uvm_component_utils(good_test_c)
  good_driver_c drv;

  function new(string name, uvm_component parent);
    super.new(name, parent);
  endfunction

  function void build_phase(uvm_phase phase);
    int timeout_val = 5000;
    super.build_phase(phase);
    uvm_config_db#(int)::set(this, "*", "timeout_ns", timeout_val);
    drv = good_driver_c::type_id::create("drv", this);
  endfunction
endclass
