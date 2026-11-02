// SV-RND-025 (DistWideRangeColonEq): compliant dists
`include "uvm_mock.svh"

class rnd025_good_item extends uvm_sequence_item;
    rand int unsigned delay;
    rand bit [7:0] len;
    rand bit [7:0] kind;
    rand bit [7:0] few;
    constraint delay_c { delay dist { 0 := 50, [1:255] :/ 50 }; }          // weight shared over the range
    constraint len_c   { len dist { [0:63] := 1, [64:127] := 1, [128:191] := 2 }; } // same-width ranges
    constraint kind_c  { kind dist { 0 := 1, [1:16] := 1 }; }               // narrow range, at the limit
    constraint few_c { few dist { 1 := 3, 2 := 1, 3 := 1 }; }
    function new(string name = "rnd025_good_item");
        super.new(name);
    endfunction
endclass
