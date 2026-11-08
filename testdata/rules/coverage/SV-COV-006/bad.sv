// Test Case: SV-COV-006 (CovergroupCrossBinExplosion)
// Extracted from sample_tb.sv (Test Case 137)

`include "uvm_mock.svh"

// --- TEST CASE 137: SV-COV-006 (CovergroupCrossBinExplosion) ---
class CrossExplosionBench;
    bit [3:0] src_id;
    bit [3:0] dst_id;
    bit [3:0] pkt_type;

    covergroup cg_burst;
        cp_src: coverpoint src_id;
        cp_dst: coverpoint dst_id;
        cp_type: coverpoint pkt_type;
        // VIOLATION SV-COV-006: 3-way cross product without ignore_bins creates combinatorial bin explosion!
        cross_3way: cross cp_src, cp_dst, cp_type;
    endgroup

    function new();
        cg_burst = new();
    endfunction
endclass
