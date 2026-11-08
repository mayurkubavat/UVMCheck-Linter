// SV-DSN-012 (PrimitiveObsession): enums, ranges and real quantities
`include "uvm_mock.svh"

typedef enum bit [1:0] {FIXED, INCR, WRAP} dsn012_burst_e;

class dsn012_good_item extends uvm_sequence_item;
    rand dsn012_burst_e burst_type;
    rand bit [7:0] len;
    rand bit [7:0] size;
    constraint burst_c { burst_type inside {INCR, WRAP}; }
    constraint len_c   { len inside {[1:16]}; }             // a range is a quantity
    constraint size_c  { size inside {1, 2}; }              // two values: too few to call an enum
    function new(string name = "dsn012_good_item");
        super.new(name);
    endfunction
endclass
