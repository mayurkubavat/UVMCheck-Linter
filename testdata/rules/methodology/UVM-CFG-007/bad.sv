// UVM-CFG-007: violations are marked.
`include "uvm_mock.svh"
class bus_driver extends uvm_driver;
  `uvm_component_utils(bus_driver)
  int delay;
  function new(string name, uvm_component parent);
    super.new(name, parent);
  endfunction
  task run_phase(uvm_phase phase);
    void'(uvm_config_db#(int)::get(this, "", "delay", delay)); // VIOLATION
    forever begin
      uvm_config_db#(int)::wait_modified(this, "", "delay"); // VIOLATION
    end
  endtask
  task main_phase(uvm_phase phase);
    uvm_config_db#(int)::set(this, "*", "busy", 1); // VIOLATION
  endtask
endclass

class bus_seq extends uvm_sequence;
  `uvm_object_utils(bus_seq)
  int len;
  function new(string name = "bus_seq");
    super.new(name);
  endfunction
  task body();
    repeat (10) begin
      void'(uvm_config_db#(int)::get(null, "seq", "len", len)); // VIOLATION
    end
  endtask
endclass
