// SV-DSN-001: behavior picked by comparing type names with strings.
`include "uvm_mock.svh"

class bus_item extends uvm_sequence_item;
  `uvm_object_utils(bus_item)
  function new(string name = "bus_item");
    super.new(name);
  endfunction
endclass

class bus_predictor extends uvm_component;
  `uvm_component_utils(bus_predictor)
  int rd, wr;
  function new(string name, uvm_component parent);
    super.new(name, parent);
  endfunction

  function void predict(bus_item t);
    if (t.get_type_name() == "rd_item") rd++; // VIOLATION
    else if ("wr_item" == t.get_type_name()) wr++; // VIOLATION
    case (t.get_type_name()) // VIOLATION
      "rd_item": rd++;
      default: ;
    endcase
  endfunction
endclass
