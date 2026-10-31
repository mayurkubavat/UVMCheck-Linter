`include "uvm_mock.svh"
class pkt extends uvm_sequence_item;
  `uvm_object_utils(pkt)
  function new(string name = "pkt"); super.new(name); endfunction
endclass : pkt

class my_drv extends uvm_driver #(pkt);
  `uvm_component_utils(my_drv)
  int depth;
  function new(string name, uvm_component parent); super.new(name, parent); endfunction
  virtual function void build_phase(uvm_phase phase);
    super.build_phase(phase);
    void'(uvm_config_db#(int)::get(this, "", "depth", depth));
    void'(uvm_config_db#(int)::get(this, "", "timeout", depth));
  endfunction : build_phase
endclass : my_drv

class my_mon extends uvm_monitor;
  `uvm_component_utils(my_mon)
  uvm_analysis_port #(pkt) ap;
  uvm_analysis_port #(pkt) dbg_ap;
  function new(string name, uvm_component parent); super.new(name, parent); endfunction
  virtual function void build_phase(uvm_phase phase);
    super.build_phase(phase);
    ap = new("ap", this);
    dbg_ap = new("dbg_ap", this);
  endfunction : build_phase
endclass : my_mon

class my_agent extends uvm_agent;
  `uvm_component_utils(my_agent)
  my_drv drv;
  my_mon mon;
  function new(string name, uvm_component parent); super.new(name, parent); endfunction
  virtual function void build_phase(uvm_phase phase);
    super.build_phase(phase);
    drv = my_drv::type_id::create("drv", this);
    mon = my_mon::type_id::create("mon", this);
  endfunction : build_phase
endclass : my_agent

class my_scb extends uvm_scoreboard;
  `uvm_component_utils(my_scb)
  uvm_analysis_imp #(pkt, my_scb) imp;
  function new(string name, uvm_component parent); super.new(name, parent); imp = new("imp", this); endfunction
  function void write(pkt t); endfunction : write
endclass : my_scb

class my_env extends uvm_env;
  `uvm_component_utils(my_env)
  my_agent agt;
  my_scb scb;
  function new(string name, uvm_component parent); super.new(name, parent); endfunction
  virtual function void build_phase(uvm_phase phase);
    super.build_phase(phase);
    agt = my_agent::type_id::create("agt", this);
    scb = my_scb::type_id::create("scb", this);
  endfunction : build_phase
  virtual function void connect_phase(uvm_phase phase);
    agt.mon.ap.connect(scb.imp);
  endfunction : connect_phase
endclass : my_env

class my_test extends uvm_test;
  `uvm_component_utils(my_test)
  my_env env;
  function new(string name, uvm_component parent); super.new(name, parent); endfunction
  virtual function void build_phase(uvm_phase phase);
    super.build_phase(phase);
    uvm_config_db#(int)::set(this, "env.agt.drv", "depth", 4);
    uvm_config_db#(int)::set(this, "env", "unused_knob", 1);
    env = my_env::type_id::create("env", this);
  endfunction : build_phase
endclass : my_test
