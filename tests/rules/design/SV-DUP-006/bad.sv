// SV-DUP-006 (DataClumps): the same four arguments travel together
`include "uvm_mock.svh"

class dup006_driver;
    task drive_write(bit [31:0] addr, bit [63:0] data, bit [7:0] strb, bit [3:0] id); endtask
endclass

class dup006_monitor;
    function void record(bit [31:0] addr, bit [63:0] data, bit [7:0] strb, bit [3:0] id); endfunction
endclass

class dup006_scb;
    function void expect_write(bit [3:0] id, bit [31:0] addr, bit [63:0] data, bit [7:0] strb); endfunction
endclass
