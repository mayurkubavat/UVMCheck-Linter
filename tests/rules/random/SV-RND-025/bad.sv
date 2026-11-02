// SV-RND-025 (DistWideRangeColonEq): := on a wide range next to single values
`include "uvm_mock.svh"

class rnd025_item extends uvm_sequence_item;
    rand int unsigned delay;
    rand bit [7:0] len;
    localparam int MAX_LEN = 200;
    constraint delay_c { delay dist { 0 := 50, [1:255] := 50 }; }
    constraint len_c   { len dist { 1 := 10, [2:MAX_LEN] := 1, 255 := 5 }; }
    function new(string name = "rnd025_item");
        super.new(name);
    endfunction
endclass
