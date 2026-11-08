// Test Case: RAL-COL-001 (RalAddressCollision)
// Extracted from sample_tb.sv (Test Case 77)

`include "uvm_mock.svh"

// --- TEST CASE 77: RAL-COL-001 (Register Address Offset Collision in uvm_reg_block) ---
class collision_reg_block extends uvm_reg_block;
    mock_reg reg_alpha;
    mock_reg reg_beta;

    function new(string name = "collision_reg_block");
        super.new(name);
    endfunction

    virtual function void build();
        reg_alpha = mock_reg::type_id::create("reg_alpha");
        reg_beta = mock_reg::type_id::create("reg_beta");
        default_map.add_reg(reg_alpha, 'h10, "RW");
        // VIOLATION RAL-COL-001: reg_beta collides with reg_alpha at identical address offset 'h10!
        default_map.add_reg(reg_beta, 'h10, "RW");
    endfunction
endclass
