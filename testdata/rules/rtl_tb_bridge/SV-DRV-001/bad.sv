// Test Case: SV-DRV-001 (DrivingDuringReset)
// Extracted from sample_tb.sv (Test Case 69)

`include "uvm_mock.svh"

// --- TEST CASE 69: SV-DRV-001 (Driving Interface During Reset) ---
interface mock_vif(input logic clk);
    logic valid;
    logic [7:0] data;
endinterface

class UnguardedDriver extends uvm_driver;
    `uvm_component_utils(UnguardedDriver)
    virtual mock_vif vif;

    function new(string name, uvm_component parent);
        super.new(name, parent);
    endfunction

    // VIOLATION SV-DRV-001: Driving virtual interface without checking hardware reset!
    task drive_pkt();
        vif.valid <= 1'b1;
    endtask
endclass
