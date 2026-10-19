// Test Case: SV-DED-007 (UnusedPureRoutineResult)
// Extracted from sample_tb.sv (Test Case 129)

`include "uvm_mock.svh"

// --- TEST CASE 129: SV-DED-007 (UnusedPureRoutineResult) ---
class UnusedPureRoutineBench;
    function void process_packet(int pkt_id);
        // VIOLATION SV-DED-007: Calling $sformatf as standalone statement discards string!
        $sformatf("Packet received with ID %0d", pkt_id);
    endfunction
endclass
