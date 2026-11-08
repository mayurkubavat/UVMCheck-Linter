// SV-RND-027 (SoftConstraintShadowing): soft constraints whose winner is hidden
`include "uvm_mock.svh"

class rnd027_base extends uvm_sequence_item;
    rand bit [7:0] len;
    rand bit [3:0] burst;
    constraint len_default_c { soft len == 4; }
    function new(string name = "rnd027_base");
        super.new(name);
    endfunction
endclass

class rnd027_item extends rnd027_base;
    constraint len_override_c { soft len == 8; }
    constraint burst_a_c { soft burst == 1; }
    constraint burst_b_c { soft burst == 2; }
    function new(string name = "rnd027_item");
        super.new(name);
    endfunction
endclass

class rnd027_seq extends uvm_sequence #(rnd027_item);
    function new(string name = "rnd027_seq");
        super.new(name);
    endfunction
    task body();
        rnd027_item req = new("req");
        if (!req.randomize() with { soft len == 16; }) $fatal(1, "randomize failed");
    endtask
endclass
