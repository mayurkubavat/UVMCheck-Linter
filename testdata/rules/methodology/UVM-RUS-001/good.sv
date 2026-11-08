// UVM-RUS-001: compliant code.
`include "uvm_mock.svh"
class bus_item extends uvm_sequence_item;
  `uvm_object_utils(bus_item)
  function new(string name = "bus_item");
    super.new(name);
  endfunction
endclass

class bus_driver extends uvm_driver #(bus_item);
  `uvm_component_utils(bus_driver)
  function new(string name, uvm_component parent);
    super.new(name, parent);
  endfunction
endclass

class bus_monitor extends uvm_monitor;
  `uvm_component_utils(bus_monitor)
  function new(string name, uvm_component parent);
    super.new(name, parent);
  endfunction
endclass

typedef uvm_sequencer #(bus_item) bus_sequencer;
class bus_agent extends uvm_agent;
  `uvm_component_utils(bus_agent)
  bus_driver drv;
  bus_sequencer sqr;
  bus_monitor mon;
  function new(string name, uvm_component parent);
    super.new(name, parent);
  endfunction
  function void build_phase(uvm_phase phase);
    super.build_phase(phase);
    mon = bus_monitor::type_id::create("mon", this);
    if (get_is_active() == UVM_ACTIVE) begin
      drv = bus_driver::type_id::create("drv", this);
      sqr = bus_sequencer::type_id::create("sqr", this);
    end
  endfunction
endclass

// Early return on passive mode guards everything after it.
class bus_agent2 extends uvm_agent;
  `uvm_component_utils(bus_agent2)
  bus_driver drv;
  function new(string name, uvm_component parent);
    super.new(name, parent);
  endfunction
  function void build_phase(uvm_phase phase);
    super.build_phase(phase);
    if (is_active == UVM_PASSIVE) return;
    drv = bus_driver::type_id::create("drv", this);
  endfunction
endclass
