// Test Case: SV-CON-012 (ForkLoopVariableCapture)
// Extracted from sample_tb.sv (Test Case 200)

`include "uvm_mock.svh"

// --- TEST CASE 200: SV-CON-012 (ForkLoopVariableCapture) ---
class ForkLoopCaptureBench;
    task spawn_workers();
        for (int i = 0; i < 4; i++) begin
            fork
                // VIOLATION SV-CON-012: Loop index 'i' captured directly by reference in fork..join_none!
                #10 $display("Worker ID: %0d", i);
            join_none
        end
    endtask : spawn_workers
endclass : ForkLoopCaptureBench
