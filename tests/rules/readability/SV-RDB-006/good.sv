// SV-RDB-006 (DoubleNegativeFlag): positive flags read straight
`include "uvm_mock.svh"

class rdb006_good_cfg extends uvm_object;
    bit en_parity_chk;
    bit disable_retry;
    bit [3:0] no_of_lanes;
    function new(string name = "rdb006_good_cfg");
        super.new(name);
    endfunction
endclass

class rdb006_good_drv;
    rdb006_good_cfg cfg;
    function void go();
        if (cfg.en_parity_chk) $display("check parity");
        if (cfg.disable_retry) $display("no retry");     // not negated
        if (~cfg.no_of_lanes == 0) $display("all lanes"); // bitwise on a vector, not a flag test
    endfunction
endclass
