// UVM-CFG-008 helper: the same key typed again.
class bus_mon extends uvm_monitor;
  `uvm_component_utils(bus_mon)
  int h;
  function new(string name, uvm_component parent);
    super.new(name, parent);
  endfunction
  function void build_phase(uvm_phase phase);
    void'(uvm_config_db#(int)::get(this, "", "vif_handle", h));
  endfunction
endclass
