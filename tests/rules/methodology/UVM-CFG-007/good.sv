// UVM-CFG-007: config_db in the build phases, in build-time loops, and once in a sequence.
`include "uvm_mock.svh"
class bus_agent extends uvm_agent;
  `uvm_component_utils(bus_agent)
  function new(string name, uvm_component parent);
    super.new(name, parent);
  endfunction
endclass

class bus_env extends uvm_env;
  `uvm_component_utils(bus_env)
  bus_agent agt[4];
  int delay;
  function new(string name, uvm_component parent);
    super.new(name, parent);
  endfunction
  function void build_phase(uvm_phase phase);
    void'(uvm_config_db#(int)::get(this, "", "delay", delay));
    foreach (agt[i]) begin
      uvm_config_db#(int)::set(this, $sformatf("agt%0d", i), "id", i);
      agt[i] = bus_agent::type_id::create($sformatf("agt%0d", i), this);
    end
  endfunction
endclass

class bus_seq extends uvm_sequence;
  `uvm_object_utils(bus_seq)
  int len;
  function new(string name = "bus_seq");
    super.new(name);
  endfunction
  task body();
    void'(uvm_config_db#(int)::get(null, "seq", "len", len));
    repeat (len) #1;
  endtask
endclass
