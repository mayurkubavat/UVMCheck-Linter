// Test Case: SV-FSM-001 (NonExhaustiveEnumCase)
// Extracted from sample_tb.sv (Test Case 81)

`include "uvm_mock.svh"

// --- TEST CASE 81: SV-FSM-001 (Non-Exhaustive Enum Case Statement with Default Arm) ---
typedef enum bit [1:0] {
    STATE_IDLE  = 2'b00,
    STATE_READ  = 2'b01,
    STATE_WRITE = 2'b10,
    STATE_ERROR = 2'b11
} fsm_protocol_state_e;

class FsmStateDecoder;
    function void decode(fsm_protocol_state_e curr_state);
        // VIOLATION SV-FSM-001: case statement ignores STATE_WRITE and STATE_ERROR while hiding behind default: arm!
        case (curr_state)
            STATE_IDLE: $display("Handling IDLE");
            STATE_READ: $display("Handling READ");
            default:    $display("Unhandled state");
        endcase
    endfunction
endclass
