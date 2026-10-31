`include "uvm_mock.svh"

class cfg_env extends uvm_env;
  `uvm_component_utils(cfg_env)
  int timeout;
  function new(string name, uvm_component parent); super.new(name, parent); endfunction
  function void build_phase(uvm_phase phase);
    uvm_config_db#(int)::set(this, "*", "timeout", 10);
    uvm_config_db#(int)::set(this, "*", "retries", 3);
    void'(uvm_config_db#(int)::get(this, "", "timeout", timeout));
  endfunction
endclass
