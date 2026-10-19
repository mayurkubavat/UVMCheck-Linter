// Testcase: UVM-TLM-004 bad.sv
// Driver pull-port declared but never connected in connect_phase
`include "uvm_mock.svh"

class my_packet extends uvm_sequence_item;
  `uvm_object_utils(my_packet)
  function new(string name="my_packet");
    super.new(name);
  endfunction
endclass

class my_driver extends uvm_component;
  `uvm_component_utils(my_driver)
  uvm_seq_item_pull_port #(my_packet) seq_item_port;

  function new(string name, uvm_component parent);
    super.new(name, parent);
    seq_item_port = new("seq_item_port", this);
  endfunction
endclass

class my_agent extends uvm_component;
  `uvm_component_utils(my_agent)
  my_driver drv;

  function new(string name, uvm_component parent);
    super.new(name, parent);
  endfunction

  function void build_phase(uvm_phase phase);
    super.build_phase(phase);
    drv = my_driver::type_id::create("drv", this);
  endfunction

  function void connect_phase(uvm_phase phase);
    super.connect_phase(phase);
    // VIOLATION: drv.seq_item_port is never connected to sequencer export!
  endfunction
endclass
