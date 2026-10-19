// Test Case: SV-OOP-007 (StaticMemberInUvmComponent)
// Extracted from sample_tb.sv (Test Case 28)

`include "uvm_mock.svh"

// --- TEST CASE 28: SV-OOP-007 (Non-const Static Member in uvm_component) ---
class MultiChannelMonitor extends uvm_component;
    `uvm_component_utils(MultiChannelMonitor)
    // VIOLATION SV-OOP-007: Non-const static variable shared across all monitor instances!
    static int m_global_packet_counter = 0;

    function new(string name, uvm_component parent);
        super.new(name, parent);
    endfunction
endclass
