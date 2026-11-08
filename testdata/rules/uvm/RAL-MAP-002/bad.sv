// Test for RAL-MAP-002: Address interval overlap in uvm_reg_map
`include "uvm_mock.svh"

class my_reg64 extends uvm_reg;
    function new(string name = "my_reg64");
        super.new(name, 8); // 8 bytes: offset 0x10 -> span 0x10..0x17
    endfunction
endclass

class my_reg32 extends uvm_reg;
    function new(string name = "my_reg32");
        super.new(name, 4); // 4 bytes: offset 0x14 -> span 0x14..0x17 (COLLISION with my_reg64!)
    endfunction
endclass

class my_reg_block_soc extends uvm_reg_block;
    `uvm_object_utils(my_reg_block_soc)

    my_reg64 ctrl_reg64;
    my_reg32 status_reg32;

    function new(string name = "");
        super.new(name);
    endfunction

    virtual function void build();
        ctrl_reg64 = new("ctrl_reg64", 8);
        status_reg32 = new("status_reg32", 4);

        default_map.add_reg(ctrl_reg64, 'h10, "RW");
        // VIOLATION: status_reg32 at 'h14 overlaps with 8-byte ctrl_reg64 at 'h10 ('h10..'h17)
        default_map.add_reg(status_reg32, 'h14, "RO");
    endfunction
endclass
