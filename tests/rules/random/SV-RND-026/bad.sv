// SV-RND-026 (UnrelatedSolveBefore): ordering between variables no constraint ties together
`include "uvm_mock.svh"

class rnd026_item extends uvm_sequence_item;
    rand bit [7:0] pkt_id;
    rand bit [7:0] payload_size;
    rand bit       is_err;
    rand bit [3:0] prio;
    constraint size_c { payload_size inside {[1:64]}; }
    constraint id_c   { pkt_id != 0; }
    constraint order_c {
        solve pkt_id before payload_size;
        solve prio before is_err;
    }
    function new(string name = "rnd026_item");
        super.new(name);
    endfunction
endclass
