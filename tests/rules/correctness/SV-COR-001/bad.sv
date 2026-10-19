// Test Case: SV-COR-001 (TautologicalCheckCompare)
// Extracted from sample_tb.sv (Test Case 61)

`include "uvm_mock.svh"

// --- TEST CASE 61: SV-COR-001 (Tautological Check Compare) ---
class TautologicalCompareTester;
    int payload;
    function void check_packet(TautologicalCompareTester act_pkt);
        // VIOLATION SV-COR-001: Comparing act_pkt.payload to itself!
        if (act_pkt.payload == act_pkt.payload) begin
            $display("Tautology detected");
        end
    endfunction
endclass
