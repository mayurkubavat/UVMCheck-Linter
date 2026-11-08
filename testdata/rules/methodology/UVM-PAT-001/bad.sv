// UVM-PAT-001: violations are marked.
`include "uvm_mock.svh"
class bus_driver_cb extends uvm_callback;
  function new(string name = "bus_driver_cb");
    super.new(name);
  endfunction
  virtual task pre_drive();
  endtask
endclass

class bus_driver extends uvm_component;
  `uvm_component_utils(bus_driver)
  function new(string name, uvm_component parent);
    super.new(name, parent);
  endfunction
  task run_phase(uvm_phase phase);
    `uvm_do_callbacks(bus_driver, bus_driver_cb, pre_drive()) // VIOLATION
  endtask
endclass
