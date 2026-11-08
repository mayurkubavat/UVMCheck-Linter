// Test Case: UVM-FAC-005 (GlobalFactoryOverridePollution)
`include "uvm_mock.svh"

class packet_item extends uvm_sequence_item;
  `uvm_object_utils(packet_item)
  function new(string name = "packet_item"); super.new(name); endfunction
endclass

class bad_packet_item extends packet_item;
  `uvm_object_utils(bad_packet_item)
  function new(string name = "bad_packet_item"); super.new(name); endfunction
endclass

class my_driver extends uvm_driver#(packet_item);
  `uvm_component_utils(my_driver)
  function new(string name, uvm_component parent); super.new(name, parent); endfunction

  virtual function void build_phase(uvm_phase phase);
    super.build_phase(phase);
    // VIOLATION: a driver changes packet_item for every test
    set_type_override("packet_item", "bad_packet_item");
  endfunction
endclass

// A VIP sub-env is reusable code too.
class vip_env extends uvm_env;
  `uvm_component_utils(vip_env)
  function new(string name, uvm_component parent); super.new(name, parent); endfunction

  virtual function void build_phase(uvm_phase phase);
    super.build_phase(phase);
    packet_item::type_id::set_type_override(bad_packet_item::type_id::get());
  endfunction
endclass

class my_agent extends uvm_agent;
  `uvm_component_utils(my_agent)
  function new(string name, uvm_component parent); super.new(name, parent); endfunction

  virtual function void build_phase(uvm_phase phase);
    uvm_factory f = uvm_factory::get();
    super.build_phase(phase);
    f.set_type_override_by_name("packet_item", "bad_packet_item");
  endfunction
endclass
