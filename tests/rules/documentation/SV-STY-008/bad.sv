// Test Case: SV-STY-008 (SectionHeaderBanner)
// Extracted from sample_tb.sv (Test Case 43)

`include "uvm_mock.svh"

// --- TEST CASE 43: SV-STY-007 & SV-STY-008 (Ragged Separator and Section Banners) ---
// VIOLATION SV-STY-008: Ragged section banner width (20 characters instead of 70-80)
//==================
// Section: Helpers
//==================

// VIOLATION SV-STY-007: Ragged method separator banner (16 dashes instead of 70-80)
//----------------
function void ragged_banner_demo();
    $display("Demo banner");
endfunction : ragged_banner_demo
