// UVM-CFG-008: the key "vif_handle" is typed in this file and in the included helper.
`include "uvm_mock.svh"
`include "bus_mon_part.svh"
class bus_env extends uvm_env;
  `uvm_component_utils(bus_env)
  int cnt;
  function new(string name, uvm_component parent);
    super.new(name, parent);
  endfunction
  function void build_phase(uvm_phase phase);
    uvm_config_db#(int)::set(this, "mon", "vif_handle", 3); // VIOLATION
    uvm_config_db#(int)::set(this, "mon", "local_only", 1);
    void'(uvm_config_db#(int)::get(this, "", "local_only", cnt));
  endfunction
endclass
