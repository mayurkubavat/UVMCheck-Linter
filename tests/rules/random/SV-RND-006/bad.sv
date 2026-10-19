// Test Case: SV-RND-006 (RandcMemoryExplosion)
// Extracted from sample_tb.sv (Test Case 67)

`include "uvm_mock.svh"

// --- TEST CASE 67: SV-RND-006 (Massive Randc Permutation Memory Explosion) ---
class RandcExplosionItem;
    // VIOLATION SV-RND-006: 32-bit randc requires 4.2 billion permutation tracking table!
    randc bit [31:0] huge_addr;
endclass
