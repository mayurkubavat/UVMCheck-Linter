// Test Case: SV-ASC-001 (AssociativeArrayMutatingRead)
// Extracted from sample_tb.sv (Test Case 126)

`include "uvm_mock.svh"

// --- TEST CASE 126: SV-ASC-001 (AssociativeArrayMutatingRead) ---
class AssocReadBench;
    int scores[int];
    function bit check_score(int player_id);
        // VIOLATION SV-ASC-001: Direct read without exists() mutates associative array!
        if (scores[player_id] == 100) begin
            return 1;
        end
        return 0;
    endfunction
endclass
