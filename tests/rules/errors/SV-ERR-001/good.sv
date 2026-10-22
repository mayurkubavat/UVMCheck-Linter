// SV-ERR-001 good: failures raise errors, or are handled explicitly.
`include "uvm_mock.svh"

class bus_item extends uvm_sequence_item;
  `uvm_object_utils(bus_item)
  rand bit [31:0] addr;
  function new(string name = "bus_item");
    super.new(name);
  endfunction
endclass

class bus_seq extends uvm_sequence #(bus_item);
  `uvm_object_utils(bus_seq)
  int timeout;
  function new(string name = "bus_seq");
    super.new(name);
  endfunction

  virtual task body();
    bus_item tr = new("tr");
    if (!tr.randomize())
      `uvm_error("RND", "randomize failed")
    if (!uvm_config_db#(int)::get(null, "", "timeout", timeout))
      timeout = 100;  // documented default
    if (!uvm_config_db#(int)::get(null, "", "timeout", timeout))
      `uvm_warning("CFG", "timeout not set, using 100")
    assert (tr.randomize()) else `uvm_fatal("RND", "randomize failed")
  endtask
endclass
