// Test Case: SV-OOP-008 (NonVirtualInterfaceInClass)
// Extracted from sample_tb.sv (Test Case 157)

`include "uvm_mock.svh"

// --- TEST CASE 157: SV-OOP-008 (NonVirtualInterfaceInClass) ---
class NonVirtualIfBench;
    // VIOLATION SV-OOP-008: Interface handle in class declared without 'virtual' keyword!
    mock_bus_if bus_if;
endclass
