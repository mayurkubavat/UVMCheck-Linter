// Test Case: SV-OOP-010 (ClassInstantiationInLoop)
// Extracted from sample_tb.sv (Test Case 174)

`include "uvm_mock.svh"

// --- TEST CASE 174: SV-OOP-010 (ClassInstantiationInLoop) ---
class NewInLoopBench;
    task generate_items();
        ShadowItem tr;
        for (int i = 0; i < 100; i++) begin
            // VIOLATION SV-OOP-010: Instantiating objects via new() inside a loop!
            tr = new();
        end
    endtask
endclass
