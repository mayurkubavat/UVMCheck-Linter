// Test Case: SV-TYP-008 (RedundantCastToSameType)
// Extracted from sample_tb.sv (Test Case 197)

`include "uvm_mock.svh"

class BasePacket;
endclass

// --- TEST CASE 197: SV-TYP-008 (RedundantCastToSameType) ---
class RedundantCastBench;
    BasePacket p1, p2;
    function void test_redundant_cast();
        p2 = new();
        // VIOLATION SV-TYP-008: $cast between identical static types 'BasePacket'!
        void'($cast(p1, p2));
    endfunction : test_redundant_cast
endclass : RedundantCastBench
