// Test Case: SV-EVT-001 (ZeroDelayEventWaitLoop)
// Extracted from sample_tb.sv (Test Case 26)

`include "uvm_mock.svh"

// --- TEST CASE 26: SV-EVT-001 (Zero-Delay Event Wait Loop) ---
module event_deadlock_detector;
    event tick_event;

    initial begin
        // VIOLATION SV-EVT-001: wait(tick_event.triggered) in loop without time advance (@ or #) causes infinite zero-delay hang!
        forever begin
            wait(tick_event.triggered);
            $display("Tick triggered");
        end
    end
endmodule
