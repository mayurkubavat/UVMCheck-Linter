// Test Case: RAL-ACC-002 (MissingBackdoorAccessPathDefinition)
// Extracted from sample_tb.sv (Test Case 222)

`include "uvm_mock.svh"

// --- TEST CASE 222: RAL-ACC-002 (MissingBackdoorAccessPathDefinition) ---
class MockRalBlock;
    function void add_hdl_path(string path); endfunction : add_hdl_path
endclass : MockRalBlock

class BackdoorEmptyPathBench;
    MockRalBlock blk;
    function void configure();
        blk = new();
        // VIOLATION RAL-ACC-002: add_hdl_path called with empty path string!
        blk.add_hdl_path("");
    endfunction : configure
endclass : BackdoorEmptyPathBench
