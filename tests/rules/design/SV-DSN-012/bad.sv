// SV-DSN-012 (PrimitiveObsession): bit vectors used as enumerations
`include "uvm_mock.svh"

class dsn012_item extends uvm_sequence_item;
    rand bit [1:0] burst_type;
    rand bit [3:0] opcode;
    constraint burst_c  { burst_type inside {0, 1, 2}; }
    constraint opcode_c { opcode inside {4'h1, 4'h2, 4'h4, 4'h8}; }
    function new(string name = "dsn012_item");
        super.new(name);
    endfunction
endclass
