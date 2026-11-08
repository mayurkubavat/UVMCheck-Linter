// SV-DUP-002: the same steps pasted into two routines, with renamed variables.
`include "uvm_mock.svh"

class bus_item extends uvm_sequence_item;
  `uvm_object_utils(bus_item)
  rand bit [31:0] addr;
  rand bit [31:0] data;
  bit [7:0] parity;
  function new(string name = "bus_item");
    super.new(name);
  endfunction
endclass

class bus_driver extends uvm_driver #(bus_item);
  `uvm_component_utils(bus_driver)
  int unsigned sent;
  bit [31:0] last_addr;
  function new(string name, uvm_component parent);
    super.new(name, parent);
  endfunction

  function void drive_write(bus_item req);
    bit [7:0] p;
    p = req.data[7:0] ^ req.data[15:8] ^ req.data[23:16] ^ req.data[31:24];
    req.parity = p;
    last_addr = req.addr & 32'hFFFF_FFFC;
    sent = sent + 1;
    `uvm_info("DRV", $sformatf("write %h", req.addr), UVM_HIGH)
  endfunction

  function void drive_read(bus_item rd);
    bit [7:0] q;
    rd.data = 0;
    // Same steps as drive_write, pasted and renamed.
    q = rd.data[7:0] ^ rd.data[15:8] ^ rd.data[23:16] ^ rd.data[31:24];
    rd.parity = q;
    last_addr = rd.addr & 32'hFFFF_FFFC;
    sent = sent + 1;
    `uvm_info("DRV", $sformatf("read %h", rd.addr), UVM_HIGH)
  endfunction
endclass
