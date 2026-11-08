// Test Case: SV-MOD-001 (GlobalUnitPollution)
// Extracted from sample_tb.sv (Test Case 79)

`include "uvm_mock.svh"

// --- TEST CASE 79: SV-MOD-001 ($unit Global Scope Pollution) ---
// VIOLATION SV-MOD-001: Global variable leaked into $unit compilation scope outside any package or module!
int leaked_global_packet_id;
