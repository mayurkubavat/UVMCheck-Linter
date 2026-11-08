// Test Case: SV-RCE-001 (ZeroDelayEventTrigger)
// Extracted from sample_tb.sv (Test Case 132)

`include "uvm_mock.svh"

// --- TEST CASE 132: SV-RCE-001 (ZeroDelayEventTrigger) ---
class EventTriggerBench;
    event sync_event;
    task signal_ready();
        // VIOLATION SV-RCE-001: Blocking event trigger causes zero-delay race!
        -> sync_event;
    endtask
endclass
