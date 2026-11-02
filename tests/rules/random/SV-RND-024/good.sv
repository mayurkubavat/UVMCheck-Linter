// SV-RND-024 (RandomizeInsideAssert): compliant forms
`include "uvm_mock.svh"

class rnd024_good_item extends uvm_sequence_item;
    rand bit [7:0] addr;
    function new(string name = "rnd024_good_item");
        super.new(name);
    endfunction
endclass

class rnd024_good_seq extends uvm_sequence #(rnd024_good_item);
    function new(string name = "rnd024_good_seq");
        super.new(name);
    endfunction

    task body();
        rnd024_good_item req = new("req");
        bit ok;
        if (!req.randomize()) $fatal(1, "randomize failed");
        ok = req.randomize();
        assert(ok);              // checks a saved result; the call still runs with asserts off
        assert(req.addr < 8'h80); // no randomize in the condition
    endtask
endclass
