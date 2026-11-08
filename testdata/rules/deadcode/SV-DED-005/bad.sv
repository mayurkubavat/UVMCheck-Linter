// Test Case: SV-DED-005 (UnreadClassProperty)
// Extracted from sample_tb.sv (Test Case 139)

`include "uvm_mock.svh"

// --- TEST CASE 139: SV-DED-005 (UnreadClassProperty) ---
class UnreadPropBench;
    // VIOLATION SV-DED-005: Property written in method but never read!
    int m_dead_counter;
    function void track_event();
        m_dead_counter++;
    endfunction
endclass
