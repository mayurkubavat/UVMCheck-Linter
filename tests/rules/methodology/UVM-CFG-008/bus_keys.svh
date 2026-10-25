// UVM-CFG-008 helper: the key, declared once.
localparam string VIF_KEY = "vif_handle";
class bus_mon2 extends uvm_monitor;
  `uvm_component_utils(bus_mon2)
  int h;
  function new(string name, uvm_component parent);
    super.new(name, parent);
  endfunction
  function void build_phase(uvm_phase phase);
    void'(uvm_config_db#(int)::get(this, "", VIF_KEY, h));
  endfunction
endclass
