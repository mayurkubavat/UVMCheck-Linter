// Test Case: SV-OOP-013 (AbstractClassInstantiation)
// Extracted from sample_tb.sv (Test Case 195)

`include "uvm_mock.svh"

// --- TEST CASE 195: SV-OOP-013 (AbstractClassInstantiation) ---
virtual class AbstractBasePkt;
    int pkt_id;
endclass : AbstractBasePkt

class AbstractInstBench;
    AbstractBasePkt pkt;
    function void instantiate_bad();
        // VIOLATION SV-OOP-013: Attempt to directly instantiate abstract (virtual) class!
        pkt = new();
    endfunction : instantiate_bad
endclass : AbstractInstBench
