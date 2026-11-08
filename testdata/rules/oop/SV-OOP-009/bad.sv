// Test Case: SV-OOP-009 (EmptyVirtualMethodWithoutPure)
// Extracted from sample_tb.sv (Test Case 166)

`include "uvm_mock.svh"

// --- TEST CASE 166: SV-OOP-009 (EmptyVirtualMethodWithoutPure) ---
virtual class AbstractDriverBase;
    // VIOLATION SV-OOP-009: Empty virtual method body without pure virtual!
    virtual function void send_packet();
    endfunction
endclass
