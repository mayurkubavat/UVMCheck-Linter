// UVM-CFG-008: a shared constant across files, and literals used in one file only.
`include "uvm_mock.svh"
`include "bus_keys.svh"
class bus_env extends uvm_env;
  `uvm_component_utils(bus_env)
  int cnt;
  function new(string name, uvm_component parent);
    super.new(name, parent);
  endfunction
  function void build_phase(uvm_phase phase);
    uvm_config_db#(int)::set(this, "mon", VIF_KEY, 3);
    uvm_config_db#(int)::set(this, "mon", "local_only", 1);
    void'(uvm_config_db#(int)::get(this, "", "local_only", cnt));
  endfunction
endclass
