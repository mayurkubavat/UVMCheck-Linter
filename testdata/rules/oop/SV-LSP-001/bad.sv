// Test Case: SV-LSP-001 (LspConstraintContradiction)
// Extracted from sample_tb.sv (Test Case 138)

`include "uvm_mock.svh"

// --- TEST CASE 138: SV-LSP-001 (LspConstraintContradiction) ---
class BaseContract;
    rand int burst_len;
    constraint c_len { burst_len inside {[1:16]}; }
endclass

class DerivedBreaker extends BaseContract;
    function new();
        super.new();
        // VIOLATION SV-LSP-001: Derived class unconditionally disables inherited constraint!
        c_len.constraint_mode(0);
    endfunction
endclass
