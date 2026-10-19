// Test Case: SV-CON-006 (EventTriggerInAlwaysComb)
// Extracted from sample_tb.sv (Test Case 164)

`include "uvm_mock.svh"

// --- TEST CASE 164: SV-CON-006 (EventTriggerInAlwaysComb) ---
module comb_event_trigger_mod;
    event comb_ev;
    int comb_data;
    always_comb begin
        comb_data = 1;
        // VIOLATION SV-CON-006: Event trigger inside always_comb!
        -> comb_ev;
    end
endmodule
