// Drop-in replacement for tests/common/uvm_mock.svh that pulls in the real
// Accellera UVM library. regression.py --uvm-home puts this directory on the
// include path and compiles uvm_pkg.sv ahead of each test file.

`ifndef UVM_MOCK_SVH
`define UVM_MOCK_SVH

`include "uvm_macros.svh"
import uvm_pkg::*;

`endif
