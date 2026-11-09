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

// A fatal whose message id is built in the guard (OpenTitan's `downcast and
// dv_fatal: ID_ = `gfn = get_full_name()) still ends the path.
`define id_fatal(MSG_, ID_ = get_full_name()) \
  if (uvm_report_enabled(UVM_NONE, UVM_FATAL, ID_)) begin \
    uvm_report_fatal(ID_, MSG_, UVM_NONE, `__FILE__, `__LINE__); \
  end

class cfg_user extends uvm_component;
  `uvm_component_utils(cfg_user)
  bus_cfg cfg;
  int unsigned depth;
  function new(string name, uvm_component parent);
    super.new(name, parent);
  endfunction

  function void take(uvm_object o);
    begin
      if (!$cast(cfg, o)) begin
        `id_fatal($sformatf("not a bus_cfg: %s", o.get_name()))
      end
    end
    depth = cfg.depth;
  endfunction
endclass
