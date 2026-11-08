// SV-RDB-009 (RedundantComment): comments that say why
`include "uvm_mock.svh"

class rdb009_good_drv;
    int pkt_count;
    function void set_cfg(); endfunction
    function void go();
        // Count zero-length packets too: the MAC filter drops them silently.
        pkt_count++;
        // cfg must be in place before the first reset
        set_cfg();
    endfunction
endclass
