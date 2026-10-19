// Testcase: UVM-TLM-004 good.sv
// Driver pull-port properly connected to sequencer in connect_phase
`include "uvm_mock.svh"

class good_packet extends uvm_sequence_item;
  `uvm_object_utils(good_packet)
  function new(string name="good_packet");
    super.new(name);
  endfunction
endclass

class good_driver extends uvm_component;
  `uvm_component_utils(good_driver)
  uvm_seq_item_pull_port #(good_packet) seq_item_port;

  function new(string name, uvm_component parent);
    super.new(name, parent);
    seq_item_port = new("seq_item_port", this);
  endfunction
endclass

class good_sequencer extends uvm_component;
  `uvm_component_utils(good_sequencer)
  uvm_seq_item_pull_export #(good_packet) seq_item_export;

  function new(string name, uvm_component parent);
    super.new(name, parent);
    seq_item_export = new("seq_item_export", this);
  endfunction
endclass

class good_agent extends uvm_component;
  `uvm_component_utils(good_agent)
  good_driver drv;
  good_sequencer sqr;

  function new(string name, uvm_component parent);
    super.new(name, parent);
  endfunction

  function void build_phase(uvm_phase phase);
    super.build_phase(phase);
    drv = good_driver::type_id::create("drv", this);
    sqr = good_sequencer::type_id::create("sqr", this);
  endfunction

  function void connect_phase(uvm_phase phase);
    super.connect_phase(phase);
    drv.seq_item_port.connect(sqr.seq_item_export);
  endfunction
endclass
