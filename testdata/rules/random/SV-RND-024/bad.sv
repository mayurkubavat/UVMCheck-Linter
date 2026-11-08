// SV-RND-024 (RandomizeInsideAssert): randomize() as the condition of an assertion
`include "uvm_mock.svh"

class rnd024_item extends uvm_sequence_item;
    rand bit [7:0] addr;
    rand bit [7:0] len;
    function new(string name = "rnd024_item");
        super.new(name);
    endfunction
endclass

class rnd024_seq extends uvm_sequence #(rnd024_item);
    function new(string name = "rnd024_seq");
        super.new(name);
    endfunction

    task body();
        rnd024_item req = new("req");
        int unsigned n;
        assert(req.randomize());
        assert(req.randomize() with { addr < 8'h10; }) else $error("randomize failed");
        assume(std::randomize(n));
    endtask
endclass
