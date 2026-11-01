// A get whose field name is never set.
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
  int m_delay;
  function new(string name, uvm_component parent);
    super.new(name, parent);
  endfunction : new
  function void build_phase(uvm_phase phase);
    super.build_phase(phase);
    if (!uvm_config_db#(int)::get(this, "", "mode", m_mode)) m_mode = 0;
    if (!uvm_config_db#(int)::get(this, "", "delay", m_delay)) m_delay = 0;
  endfunction : build_phase
  function int value();
    return m_mode + m_delay;
  endfunction : value
endclass : mode_getter
