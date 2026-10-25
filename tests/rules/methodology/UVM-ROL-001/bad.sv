// UVM-ROL-001: violations are marked.
`include "uvm_mock.svh"
class bus_item extends uvm_sequence_item;
  `uvm_object_utils(bus_item)
  rand bit [31:0] addr;
  function new(string name = "bus_item");
    super.new(name);
  endfunction
endclass

class idle_seq extends uvm_sequence #(bus_item);
  `uvm_object_utils(idle_seq)
  function new(string name = "idle_seq");
    super.new(name);
  endfunction
endclass

class bus_driver extends uvm_driver #(bus_item);
  `uvm_component_utils(bus_driver)
  uvm_sequencer_base sqr;
  function new(string name, uvm_component parent);
    super.new(name, parent);
  endfunction
  task run_phase(uvm_phase phase);
    bus_item it;
    idle_seq seq;
    it = bus_item::type_id::create("it");
    if (!it.randomize()) // VIOLATION
      `uvm_error("DRV", "randomize failed")
    seq = idle_seq::type_id::create("seq");
    seq.start(sqr); // VIOLATION
  endtask
endclass
