// Test Case: SV-SML-004 (CyclomaticComplexityGodMethod)
// Extracted from sample_tb.sv (Test Case 91)

`include "uvm_mock.svh"

// --- TEST CASE 91: SV-SML-004 (CyclomaticComplexityGodMethod) ---
class GodMethodBench;
    function void spaghetti_handler(int a, int b, int c, int d, int e, int f, int g, int h);
        // VIOLATION SV-SML-004: Cyclomatic complexity > 15!
        if (a > 0 && b > 0) $display("1");
        if (c > 0 || d > 0) $display("2");
        if (e > 0 && f > 0) $display("3");
        if (g > 0 || h > 0) $display("4");
        if (a == 1) $display("5");
        if (b == 2) $display("6");
        if (c == 3) $display("7");
        if (d == 4) $display("8");
        if (e == 5) $display("9");
        if (f == 6) $display("10");
        if (g == 7) $display("11");
        if (h == 8) $display("12");
    endfunction
endclass
