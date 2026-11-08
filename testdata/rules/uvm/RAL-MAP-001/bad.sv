// Test Case: RAL-MAP-001 (RalOrphanRegister)
// Extracted from sample_tb.sv (Test Case 76)

`include "uvm_mock.svh"

// --- TEST CASE 76: RAL-MAP-001 (Orphan Register Missing Address Map in uvm_reg_block) ---
class mock_reg_type_id;
    static function mock_reg create(string name = "");
        mock_reg r = new(name);
        return r;
    endfunction
endclass

class mock_reg extends uvm_object;
    typedef mock_reg_type_id type_id;
    function new(string name = "mock_reg");
        super.new(name);
    endfunction
endclass

class uvm_reg_map;
    function void add_reg(uvm_object rg, bit [63:0] offset, string rights = "RW");
    endfunction
endclass

class uvm_reg_block extends uvm_object;
    uvm_reg_map default_map;
    function new(string name = "uvm_reg_block");
        super.new(name);
        default_map = new();
    endfunction
endclass

class orphan_reg_block extends uvm_reg_block;
    mock_reg valid_reg;
    mock_reg phantom_reg;

    function new(string name = "orphan_reg_block");
        super.new(name);
    endfunction

    virtual function void build();
        valid_reg = mock_reg::type_id::create("valid_reg");
        phantom_reg = mock_reg::type_id::create("phantom_reg");
        // VIOLATION RAL-MAP-001: phantom_reg is instantiated but never mapped via default_map.add_reg()!
        default_map.add_reg(valid_reg, 'h00, "RW");
    endfunction
endclass
