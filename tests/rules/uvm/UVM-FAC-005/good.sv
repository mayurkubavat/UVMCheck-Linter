// Compliant Clean Test Case: UVM-FAC-005 (GlobalFactoryOverridePollution)
`include "uvm_mock.svh"

class packet_item extends uvm_sequence_item;
  `uvm_object_utils(packet_item)
  function new(string name = "packet_item"); super.new(name); endfunction
endclass

class good_packet_item extends packet_item;
  `uvm_object_utils(good_packet_item)
  function new(string name = "good_packet_item"); super.new(name); endfunction
endclass

// In a test, global factory overrides are fully compliant
class my_test extends uvm_test;
  `uvm_component_utils(my_test)
  function new(string name, uvm_component parent); super.new(name, parent); endfunction

  virtual function void build_phase(uvm_phase phase);
    super.build_phase(phase);
    set_type_override("packet_item", "good_packet_item");
  endfunction
endclass

// Instance overrides scoped to the component's own sub-tree are fine anywhere.
class scoped_env extends uvm_env;
  `uvm_component_utils(scoped_env)
  function new(string name, uvm_component parent); super.new(name, parent); endfunction

  virtual function void build_phase(uvm_phase phase);
    super.build_phase(phase);
    set_inst_override("agent.*", "packet_item", "good_packet_item");
  endfunction
endclass
