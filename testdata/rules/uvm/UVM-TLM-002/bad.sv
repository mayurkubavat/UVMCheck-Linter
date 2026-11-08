// Test Case: UVM-TLM-002 (UvmTlmTryGetUnchecked)
// Extracted from sample_tb.sv (Test Case 159)

`include "uvm_mock.svh"

// --- TEST CASE 159: UVM-TLM-002 (UvmTlmTryGetUnchecked) ---
class UncheckedTlmBench extends uvm_component;
    `uvm_component_utils(UncheckedTlmBench)
    uvm_tlm_fifo #(int) fifo;
    function new(string name = "UncheckedTlmBench", uvm_component parent = null);
        super.new(name, parent);
    endfunction
    task run_phase(uvm_phase phase);
        int item;
        // VIOLATION UVM-TLM-002: Calling try_get() without checking return boolean!
        fifo.try_get(item);
    endtask
endclass
