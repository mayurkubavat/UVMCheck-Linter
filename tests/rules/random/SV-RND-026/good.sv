// SV-RND-026 (UnrelatedSolveBefore): solve-before on related variables
`include "uvm_mock.svh"

class rnd026_good_base extends uvm_sequence_item;
    rand bit       is_err;
    rand bit [7:0] payload_size;
    rand bit [7:0] hdr_len;
    rand bit [7:0] total;
    constraint err_c { is_err -> payload_size == 0; }
    function new(string name = "rnd026_good_base");
        super.new(name);
    endfunction
endclass

class rnd026_good_item extends rnd026_good_base;
    rand bit [1:0] mode;
    constraint total_c { total == hdr_len + payload_size; }
    constraint mode_c  { if (mode == 0) hdr_len == 4; else hdr_len == 8; }
    constraint order_c {
        solve is_err before payload_size; // related in the base class
        solve mode before hdr_len;        // related through the if
        solve mode before total;          // related through hdr_len
    }
    function new(string name = "rnd026_good_item");
        super.new(name);
    endfunction
endclass
