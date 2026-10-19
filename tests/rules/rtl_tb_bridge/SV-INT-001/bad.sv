// Test Case: SV-INT-001 (BlockingAssignmentToInterface)
// Extracted from sample_tb.sv (Test Case 125)

`include "uvm_mock.svh"

// --- TEST CASE 125: SV-INT-001 (BlockingAssignmentToInterface) ---
interface mock_bus_if;
    logic clk;
    logic [7:0] data;
    clocking cb @(posedge clk);
        output data;
    endclocking
endinterface

class BlockingDriveBench;
    virtual mock_bus_if vif;
    task drive_bus();
        // VIOLATION SV-INT-001: Blocking assignment '=' to clocking block output!
        vif.cb.data = 8'hFF;
    endtask
endclass
