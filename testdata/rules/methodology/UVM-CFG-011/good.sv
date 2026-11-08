// The set and the get use the same field name.
`include "uvm_mock.svh"

class mode_setter extends uvm_component;
  `uvm_component_utils(mode_setter)
  function new(string name, uvm_component parent);
    super.new(name, parent);
  endfunction : new
  function void build_phase(uvm_phase phase);
    super.build_phase(phase);
    uvm_config_db#(int)::set(this, "", "mode", 0);
  endfunction : build_phase
endclass : mode_setter

class mode_getter extends uvm_component;
  `uvm_component_utils(mode_getter)
  int m_mode;
  function new(string name, uvm_component parent);
    super.new(name, parent);
  endfunction : new
  function void build_phase(uvm_phase phase);
    super.build_phase(phase);
    if (!uvm_config_db#(int)::get(this, "", "mode", m_mode)) m_mode = 0;
  endfunction : build_phase
  function int value();
    return m_mode;
  endfunction : value
endclass : mode_getter
