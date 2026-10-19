// Test Case: SV-STY-004 (TypedefNameSuffix)
// Extracted from sample_tb.sv (Test Case 41)

`include "uvm_mock.svh"

// --- TEST CASE 41: SV-STY-004 (Typedef Suffix Convention '_e' and '_t') ---
// VIOLATION SV-STY-004: Enum typedef does not end with '_e'!
typedef enum {
    STATE_RESET,
    STATE_BUSY
} fsm_state;

// VIOLATION SV-STY-004: Struct typedef does not end with '_t'!
typedef struct {
    int packet_id;
    int payload_size;
} packet_header_info;
