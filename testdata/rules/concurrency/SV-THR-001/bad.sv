// Test Case: SV-THR-001 (ForkLoopVariableShared)
// Extracted from sample_tb.sv (Test Case 4)

`include "uvm_mock.svh"

// --- TEST CASE 4: SV-THR-001 (Fork loop variable shared by reference) ---
    task send_all_channels();
        // VIOLATION SV-THR-001: loop variable 'ch' captured in fork..join_none
        for (int ch = 0; ch < 4; ch++) begin
            fork
                $display("Channel: %0d", ch);
            join_none
        end

        // Compliant concurrency: copied to automatic local variable
        for (int ch = 0; ch < 4; ch++) begin
            automatic int local_ch = ch;
            fork
                $display("Channel: %0d", local_ch);
            join_none
        end
    endtask
endclass
