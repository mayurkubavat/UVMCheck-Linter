// Test Case: SV-DEP-006 (NonAnsiPortList)
// Extracted from sample_tb.sv (Test Case 53)

`include "uvm_mock.svh"

// --- TEST CASE 53: SV-DEP-006 (Non-ANSI Port List) ---
module non_ansi_port_mod(a, b);
    // VIOLATION SV-DEP-006: Non-ANSI module port list!
    input logic a;
    output logic b;
endmodule

module non_ansi_task_mod;
    // VIOLATION SV-DEP-006: Non-ANSI task port list declared in task body!
    task legacy_task;
        input [7:0] data;
        begin
            $display("Data: %0d", data);
        end
    endtask
endmodule
