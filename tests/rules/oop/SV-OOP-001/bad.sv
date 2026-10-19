// Test Case: SV-OOP-001 (NonVirtualMethodOverride)
// Extracted from sample_tb.sv (Test Case 1)

`define MOCK_BASE_DRIVER_DEFINED
`include "uvm_mock.svh"

// --- TEST CASE 1: SV-OOP-001 (Non-virtual method override) & SV-PRF-001 (Array by value) ---
class BaseDriver extends uvm_component;
    function new(string name, uvm_component parent);
        super.new(name, parent);
    endfunction

    // Non-virtual method in base class
    function void reset_signals();
    endfunction
endclass

class MyDriver extends BaseDriver;
    function new(string name, uvm_component parent);
        super.new(name, parent);
    endfunction

    // VIOLATION SV-OOP-001: overrides non-virtual method reset_signals
    function void reset_signals();
    endfunction

    // VIOLATION SV-PRF-001: Passing dynamic array by value (input) instead of const ref
    function void send_packet(byte data[]);
    endfunction

    // Compliant performance: passing by const ref
    function void send_packet_fast(const ref byte data[]);
    endfunction
endclass
