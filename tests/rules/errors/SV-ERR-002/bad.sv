// SV-ERR-002: `uvm_error does not stop execution, so the null handle is used anyway.
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
      `uvm_error("CFG", "no vif")
    vif.valid = 0; // VIOLATION
    if (cfg == null)
      `uvm_error("CFG", "no cfg")
    depth = cfg.depth; // VIOLATION
  endfunction
endclass
