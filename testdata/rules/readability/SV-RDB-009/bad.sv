// SV-RDB-009 (RedundantComment): comments that repeat the code
`include "uvm_mock.svh"

class rdb009_drv;
    int pkt_count;
    function void set_cfg(); endfunction
    function void reset_counters(); endfunction
    function void go();
        // set cfg
        set_cfg();
        // reset the counters
        reset_counters();
    endfunction
endclass
