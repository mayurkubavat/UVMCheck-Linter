// Test Case: SV-DED-012 (UnreferencedParameter)
// Extracted from sample_tb.sv (Test Case 189)

`include "uvm_mock.svh"

// --- TEST CASE 189: SV-DED-012 (UnreferencedParameter) ---
class UnusedParamBench;
    // VIOLATION SV-DED-012: Parameter declared but never referenced in class scope!
    localparam int UNUSED_TIMEOUT_LIMIT = 5000;
endclass
