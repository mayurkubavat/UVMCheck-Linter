// SV-RDB-006 (DoubleNegativeFlag): negated negative flags
`include "uvm_mock.svh"

class rdb006_cfg extends uvm_object;
    bit disable_parity_chk;
    bit no_retry;
    function new(string name = "rdb006_cfg");
        super.new(name);
    endfunction
endclass

class rdb006_drv;
    rdb006_cfg cfg;
    bit skip_reset;
    function void go();
        if (!cfg.disable_parity_chk) $display("check parity");
        if (~cfg.no_retry) $display("retry");
        if (!skip_reset) $display("reset");
    endfunction
endclass
