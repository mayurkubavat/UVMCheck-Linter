// SV-RDB-007 (NonDescriptiveName): names that say nothing
`include "uvm_mock.svh"

class rdb007_scb;
    int tmp;
    bit [31:0] tmp2;
    function void check_it(int expected);
        int val;
        int got;
        val = expected + 1;
        got = val * 2;
        if (got != expected) $display("mismatch");
        got = got + 1;
        got = got + 2;
        got = got + 3;
        got = got + 4;
        got = got + 5;
        $display("%0d", got);
    endfunction
endclass
