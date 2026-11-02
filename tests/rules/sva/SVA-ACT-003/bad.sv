// SVA-ACT-003 (SideEffectInAssertionAction): state changes inside assertion actions
`include "uvm_mock.svh"

module act003_checker(input logic clk, input logic req, input logic ack);
    int fail_count;
    int pass_count;
    a_req_ack: assert property (@(posedge clk) req |-> ##[1:4] ack)
        pass_count++;
    else begin
        fail_count = fail_count + 1;
        $error("ack missing");
    end
endmodule

class act003_scb;
    int errors;
    bit seen;
    function void check_it(int got, int exp);
        assert (got == exp) else errors++;
        assert (got >= 0) seen = 1'b1;
    endfunction
endclass
