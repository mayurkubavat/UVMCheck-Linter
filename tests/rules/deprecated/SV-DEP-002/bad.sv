// Test Case: SV-DEP-002 (DefparamDeprecation)
// Extracted from sample_tb.sv (Test Case 49)

`include "uvm_mock.svh"

// --- TEST CASE 49: SV-DEP-002 (Defparam Deprecation) ---
module target_param_mod #(parameter int WIDTH = 8);
endmodule

module defparam_tester;
    target_param_mod u_target();
    // VIOLATION SV-DEP-002: defparam is formally deprecated in IEEE 1800 § 23.10.1!
    defparam u_target.WIDTH = 16;
endmodule
