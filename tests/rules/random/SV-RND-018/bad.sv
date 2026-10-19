// Test Case: SV-RND-018 (RandomizeInConstructor)
// Extracted from sample_tb.sv (Test Case 185)

`include "uvm_mock.svh"

// --- TEST CASE 185: SV-RND-018 (RandomizeInConstructor) ---
class RandomizeCtorBench;
    rand int data_val;
    function new();
        // VIOLATION SV-RND-018: Calling randomize() inside class constructor breaks inheritance!
        void'(this.randomize());
    endfunction
endclass
