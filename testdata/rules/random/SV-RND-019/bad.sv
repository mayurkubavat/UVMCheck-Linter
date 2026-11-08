// Test Case: SV-RND-019 (ConstraintImplicationMultiBit)
// Extracted from sample_tb.sv (Test Case 193)

`include "uvm_mock.svh"

// --- TEST CASE 193: SV-RND-019 (ConstraintImplicationMultiBit) ---
class MultiBitImplicationBench;
    rand bit [3:0] mode;
    rand int data_val;
    constraint c_multi_bit_imp {
        // VIOLATION SV-RND-019: Multi-bit condition in constraint implication operator '->'!
        mode -> data_val > 10;
    }
endclass : MultiBitImplicationBench
