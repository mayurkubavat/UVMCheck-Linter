// UVM-RUS-005: compliant code.
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
  string override_name;
  function new(string name, uvm_component parent);
    super.new(name, parent);
  endfunction
  function void build_phase(uvm_phase phase);
    super.build_phase(phase);
    bus_driver::type_id::set_type_override(slow_driver::type_id::get());
    // A name chosen at run time (for example from a plusarg) has no typed form.
    if ($value$plusargs("DRV=%s", override_name)) set_type_override("bus_driver", override_name);
  endfunction
endclass
