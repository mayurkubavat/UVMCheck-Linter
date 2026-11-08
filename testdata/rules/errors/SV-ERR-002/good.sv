// SV-ERR-002 good: the failure path stops, or the handle gets a default.
`include "uvm_mock.svh"

interface tb_bus_if;
  logic valid;
endinterface

class bus_cfg extends uvm_object;
  `uvm_object_utils(bus_cfg)
  int unsigned depth;
  function new(string name = "bus_cfg");
    super.new(name);
  endfunction
endclass

class bus_driver extends uvm_component;
  `uvm_component_utils(bus_driver)
  virtual tb_bus_if vif;
  bus_cfg cfg;
  int unsigned depth;
  function new(string name, uvm_component parent);
    super.new(name, parent);
  endfunction

  function void build_phase(uvm_phase phase);
    super.build_phase(phase);
    if (!uvm_config_db#(virtual tb_bus_if)::get(this, "", "vif", vif))
      `uvm_fatal("CFG", "no vif")
    vif.valid = 0;
    if (cfg == null) cfg = new("cfg");
    depth = cfg.depth;
  endfunction

  function int get_depth();
    if (cfg != null) return cfg.depth;
    return 0;
  endfunction

  function void apply();
    if (cfg == null) begin
      `uvm_error("CFG", "no cfg")
      return;
    end
    depth = cfg.depth;
  endfunction
endclass
