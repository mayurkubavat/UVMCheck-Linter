// SV-RND-027 (SoftConstraintShadowing): one soft default, hard overrides
`include "uvm_mock.svh"

class rnd027_good_base extends uvm_sequence_item;
    rand bit [7:0] len;
    rand bit [3:0] burst;
    constraint len_default_c { soft len == 4; }
    constraint burst_default_c { soft burst inside {[1:4]}; }
    function new(string name = "rnd027_good_base");
        super.new(name);
    endfunction
endclass

class rnd027_good_item extends rnd027_good_base;
    constraint len_override_c { len == 8; }
    function new(string name = "rnd027_good_item");
        super.new(name);
    endfunction
endclass

class rnd027_good_seq extends uvm_sequence #(rnd027_good_item);
    function new(string name = "rnd027_good_seq");
        super.new(name);
    endfunction
    task body();
        rnd027_good_item req = new("req");
        if (!req.randomize() with { len == 16; }) $fatal(1, "randomize failed");
    endtask
endclass
