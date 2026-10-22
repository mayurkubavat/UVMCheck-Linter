// SV-DSN-001 good: type names used only in messages; dispatch through a virtual method.
`include "uvm_mock.svh"

virtual class bus_item extends uvm_sequence_item;
  function new(string name = "bus_item");
    super.new(name);
  endfunction
  pure virtual function void predict(ref int rd, ref int wr);
endclass

class bus_predictor extends uvm_component;
  `uvm_component_utils(bus_predictor)
  int rd, wr;
  function new(string name, uvm_component parent);
    super.new(name, parent);
  endfunction

  function void predict(bus_item t);
    `uvm_info("PRD", {"predicting ", t.get_type_name()}, UVM_HIGH)
    t.predict(rd, wr);
  endfunction
endclass
