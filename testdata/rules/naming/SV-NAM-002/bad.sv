// Test Case: SV-NAM-002 (PackageNamingConvention)
// Extracted from sample_tb.sv (Test Case 123)

`include "uvm_mock.svh"

// --- TEST CASE 123: SV-NAM-002 (PackageNamingConvention) ---
// VIOLATION SV-NAM-002: Package lacking _pkg suffix!
package telemetry_defs;
    typedef int counter_t;
endpackage
