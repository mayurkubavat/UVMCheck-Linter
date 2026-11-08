// Test Case: SV-LOC-001 (ArrayLocatorEmptyQueueDeref)
// Extracted from sample_tb.sv (Test Case 128)

`include "uvm_mock.svh"

// --- TEST CASE 128: SV-LOC-001 (ArrayLocatorEmptyQueueDeref) ---
class ArrayLocatorBench;
    int items[$];
    function int get_first_high();
        // VIOLATION SV-LOC-001: Immediate dereferencing [0] on locator queue without size check!
        return items.find(x) with (x > 100)[0];
    endfunction
endclass
