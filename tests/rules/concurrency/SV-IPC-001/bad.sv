// Test Case: SV-IPC-001 (TryPutIgnoredReturn)
// Extracted from sample_tb.sv (Test Case 68)

`include "uvm_mock.svh"

// --- TEST CASE 68: SV-IPC-001 (Discarded Return on Non-Blocking IPC) ---
class DiscardedIpcTester;
    uvm_tlm_fifo #(int) fifo;
    function void send_item(int item);
        // VIOLATION SV-IPC-001: Discarding return value of try_put drops packet when FIFO is full!
        void'(fifo.try_put(item));
    endfunction
endclass
