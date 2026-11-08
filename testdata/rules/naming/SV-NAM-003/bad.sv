// Test Case: SV-NAM-003 (InterfaceNamingConvention)
// Extracted from sample_tb.sv (Test Case 124)

`include "uvm_mock.svh"

// --- TEST CASE 124: SV-NAM-003 (InterfaceNamingConvention) ---
// VIOLATION SV-NAM-003: Interface lacking _if suffix!
interface custom_uart_bus;
    logic rx;
    logic tx;
endinterface
