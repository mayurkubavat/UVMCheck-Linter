// UVM-RUS-005: violations are marked.
`include "uvm_mock.svh"
class bus_driver extends uvm_component;
  `uvm_component_utils(bus_driver)
  function new(string name, uvm_component parent);
    super.new(name, parent);
  endfunction
endclass

class slow_driver extends bus_driver;
  `uvm_component_utils(slow_driver)
  function new(string name, uvm_component parent);
    super.new(name, parent);
  endfunction
endclass

class slow_test extends uvm_test;
  `uvm_component_utils(slow_test)
  function new(string name, uvm_component parent);
    super.new(name, parent);
  endfunction
  function void build_phase(uvm_phase phase);
    uvm_factory f = uvm_factory::get();
    super.build_phase(phase);
    set_type_override("bus_driver", "slow_driver"); // VIOLATION
    f.set_inst_override_by_name("bus_driver", "slow_drvier", "uvm_test_top.env.*"); // VIOLATION
  endfunction
endclass
