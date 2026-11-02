// SV-RDB-008 (UnclearAbbreviation): vowel-stripped words
`include "uvm_mock.svh"

class rdb008_chnnl_mgr;
    int trnsfr_cnt;
    function void rgstr_cb(int pkt_lngth);
    endfunction
endclass
