// Test Case: SV-OCP-001 (CastTypeInspectionLadder)
// Extracted from sample_tb.sv (Test Case 134)

`include "uvm_mock.svh"

// --- TEST CASE 134: SV-OCP-001 (CastTypeInspectionLadder) ---
class BasePkt; endclass
class PktA extends BasePkt; endclass
class PktB extends BasePkt; endclass
class PktC extends BasePkt; endclass

class CastLadderBench;
    function void dispatch_packet(BasePkt pkt);
        PktA a;
        PktB b;
        PktC c;
        // VIOLATION SV-OCP-001: Cascading $cast ladder violates Open-Closed Principle!
        if ($cast(a, pkt)) begin
            $display("Matched PktA");
        end else if ($cast(b, pkt)) begin
            $display("Matched PktB");
        end else if ($cast(c, pkt)) begin
            $display("Matched PktC");
        end
    endfunction
endclass
