// SV-DUP-002: the shared steps live in one helper.
`include "uvm_mock.svh"
// Generated files are left out.
`include "gen_csr_ops.svh"

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

  function void finish(bus_item t, string kind);
    t.parity = t.data[7:0] ^ t.data[15:8] ^ t.data[23:16] ^ t.data[31:24];
    last_addr = t.addr & 32'hFFFF_FFFC;
    sent = sent + 1;
    `uvm_info("DRV", $sformatf("%s %h", kind, t.addr), UVM_HIGH)
  endfunction

  function void drive_write(bus_item req);
    finish(req, "write");
  endfunction

  function void drive_read(bus_item rd);
    rd.data = 0;
    finish(rd, "read");
  endfunction

  // An unrolled run of one short cycle of statements is a loop, not a copied fragment.
  function void preload();
    last_addr = 32'h0100;
    sent = sent + 0;
    last_addr = 32'h0104;
    sent = sent + 1;
    last_addr = 32'h0108;
    sent = sent + 2;
    last_addr = 32'h010C;
    sent = sent + 3;
    last_addr = 32'h0110;
    sent = sent + 4;
    last_addr = 32'h0114;
    sent = sent + 5;
    last_addr = 32'h0118;
    sent = sent + 6;
    last_addr = 32'h011C;
    sent = sent + 7;
  endfunction

  // Short look-alike runs stay below the token minimum.
  function void reset_a();
    sent = 0;
    last_addr = 0;
  endfunction

  function void reset_b();
    sent = 0;
    last_addr = 0;
  endfunction
endclass
