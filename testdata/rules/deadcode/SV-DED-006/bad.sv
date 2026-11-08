// Test Case: SV-DED-006 (UnwrittenClassProperty)
// Extracted from sample_tb.sv (Test Case 140)

`include "uvm_mock.svh"

// --- TEST CASE 140: SV-DED-006 (UnwrittenClassProperty) ---
class UnwrittenPropBench;
    // VIOLATION SV-DED-006: Property read in condition but never initialized or assigned!
    int m_uninit_timeout;
    function bit is_expired(int current_time);
        if (current_time > m_uninit_timeout) begin
            return 1;
        end
        return 0;
    endfunction
endclass
