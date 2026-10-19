// Test Case: UVM-ALS-001 (UvmTransactionAliasing)
// Extracted from sample_tb.sv (Test Case 136)

`include "uvm_mock.svh"

// --- TEST CASE 136: UVM-ALS-001 (UvmTransactionAliasing) ---
class MockTransaction;
    int addr;
    int data;
endclass

class AnalysisAliasBench extends uvm_component;
    `uvm_component_utils(AnalysisAliasBench)
    uvm_analysis_port #(MockTransaction) ap;
    function new(string name = "AnalysisAliasBench", uvm_component parent = null);
        super.new(name, parent);
    endfunction
    task run_phase(uvm_phase phase);
        MockTransaction tr;
        tr = new();
        tr.addr = 32'h1000;
        ap.write(tr);
        // VIOLATION UVM-ALS-001: Mutating transaction field in-place after writing to analysis port!
        tr.data = 32'hDEAD;
    endtask
endclass
