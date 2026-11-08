// Test Case: SV-RND-014 (EmptyRandomizeWith)
// Extracted from sample_tb.sv (Test Case 146)

`include "uvm_mock.svh"

// --- TEST CASE 146: SV-RND-014 (EmptyRandomizeWith) ---
class EmptyRandomizeWithBench;
    task run_test();
        MockTransaction tr = new();
        // VIOLATION SV-RND-014: randomize() with empty inline constraint block!
        if (!tr.randomize() with {}) begin
            $display("Randomization failed");
        end
    endtask
endclass
