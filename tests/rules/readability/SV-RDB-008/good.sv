// SV-RDB-008 (UnclearAbbreviation): common DV short forms and full words
`include "uvm_mock.svh"

class rdb008_channel_mgr;
    int transfer_cnt;
    int pkts_sent;
    bit [7:0] ctrl_reg;
    bit jtag_en;
    function void register_cb(int pkt_len);
    endfunction
endclass
