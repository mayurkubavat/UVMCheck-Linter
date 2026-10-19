// Test Case: SV-TIM-001 (TimescalePrecisionMismatch)
// Extracted from sample_tb.sv (Test Case 148)

`include "uvm_mock.svh"

// --- TEST CASE 148: SV-TIM-001 (TimescalePrecisionMismatch) ---
// VIOLATION SV-TIM-001: Time precision 10ns is coarser than time unit 1ns!
`timescale 1ns / 10ns
module MockTimescaleModule;
endmodule
