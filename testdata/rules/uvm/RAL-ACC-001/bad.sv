// Test Case: RAL-ACC-001 (DirectMirrorSetWithoutWrite)
// Extracted from sample_tb.sv (Test Case 212)

`include "uvm_mock.svh"

// --- TEST CASE 212: RAL-ACC-001 (DirectMirrorSetWithoutWrite) ---
class MockReg;
    function void set(int val); endfunction : set
endclass : MockReg

class RalDirectSetBench extends uvm_sequence;
    `uvm_object_utils(RalDirectSetBench)
    MockReg ctrl_reg;
    task body();
        ctrl_reg = new();
        // VIOLATION RAL-ACC-001: reg.set() updates only software mirror without DUT write!
        ctrl_reg.set(32'hA5A5);
    endtask : body
endclass : RalDirectSetBench
