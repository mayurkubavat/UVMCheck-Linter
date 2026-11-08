// SV-DUP-006 (DataClumps): the group is a transaction; small shared argument lists are fine
`include "uvm_mock.svh"

class dup006_good_txn;
    bit [31:0] addr;
    bit [63:0] data;
    bit [7:0]  strb;
    bit [3:0]  id;
endclass

class dup006_good_driver;
    task drive_write(dup006_good_txn t); endtask
    task poke(bit [31:0] addr, bit [63:0] data); endtask
endclass

class dup006_good_monitor;
    function void record(dup006_good_txn t); endfunction
    function void peek(bit [31:0] addr, bit [63:0] data); endfunction
endclass

class dup006_good_scb;
    function void expect_write(dup006_good_txn t); endfunction
    function void note(bit [31:0] addr, bit [63:0] data); endfunction
endclass
