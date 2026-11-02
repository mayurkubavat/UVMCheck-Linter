// SVA-ACT-003 (SideEffectInAssertionAction): actions that only report
`include "uvm_mock.svh"

module act003_good_checker(input logic clk, input logic req, input logic ack);
    a_req_ack: assert property (@(posedge clk) req |-> ##[1:4] ack)
        else $error("ack missing after req");
endmodule

class act003_good_scb;
    int errors;
    function void check_it(int got, int exp);
        assert (got == exp) else $error("mismatch: got %0d exp %0d", got, exp);
        if (got != exp) errors++;   // state kept outside the assertion
    endfunction
endclass
