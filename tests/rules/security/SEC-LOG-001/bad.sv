// Test Case: SEC-LOG-001 (PlaintextKeyInLogs)
// Extracted from sample_tb.sv (Test Case 75)

`include "uvm_mock.svh"

// --- TEST CASE 75: SEC-LOG-001 (Plaintext Logging of Cryptographic Assets) ---
`define uvm_info_macro(ID, MSG, VERBOSITY) $display("[%s] %s", ID, MSG);

class SecurityLogTester extends uvm_component;
    `uvm_component_utils(SecurityLogTester)
    bit [255:0] aes_key;

    function new(string name, uvm_component parent);
        super.new(name, parent);
    endfunction

    function void build_phase(uvm_phase phase);
        super.build_phase(phase);
        // VIOLATION SEC-LOG-001: Plaintext logging of aes_key violates hardware security standards!
        `uvm_info_macro("CRYPTO", $sformatf("aes_key: %h", aes_key), 100)
    endfunction
endclass
