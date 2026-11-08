// Test Case: SV-THR-002 (StaticRoutineVariableLifetime)
// Extracted from sample_tb.sv (Test Case 22)

`include "uvm_mock.svh"

// --- TEST CASE 22: SV-THR-002 (Static Routine Variable Lifetime in Module/Package) ---
module legacy_helper_module;
    // VIOLATION SV-THR-002: Non-class task defaults to static lifetime; local variable 'temp_buf' is shared across concurrent calls!
    task process_raw_data(bit [7:0] data);
        int temp_buf;
        temp_buf = data + 1;
        #5;
    endtask
endmodule
