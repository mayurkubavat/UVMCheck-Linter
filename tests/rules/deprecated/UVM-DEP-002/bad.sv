// Test Case: UVM-DEP-002 (UvmDeprecatedReportServer)
// Extracted from sample_tb.sv (Test Case 55)

`include "uvm_mock.svh"

// --- TEST CASE 55: UVM-DEP-002 (UVM Deprecated Report Server) ---
module deprecated_report_server_tester;
    initial begin
        uvm_report_server s;
        // VIOLATION UVM-DEP-002: uvm_report_server::get_server() is deprecated!
        s = uvm_report_server::get_server();
    end
endmodule
