// Clause 10: assignments. 10.3 continuous assignment with delay and strength,
// 10.4 blocking and nonblocking, 10.6 assign/deassign and force/release,
// 10.9 assignment patterns, 10.10 unpacked array concatenation, 10.11 net aliasing.
module ch10_assignments;
  wire [7:0] a, b;
  wire (strong0, weak1) #2 c = 1'b1;
  logic [7:0] r;
  int arr [4];
  int q [$];
  alias a = b;
  assign #1 b = 8'h3;
  initial begin
    r = 8'h0;
    r <= #1 8'hF;
    arr = '{default: 0};
    arr = '{0: 1, 1: 2, default: 9};
    q = {arr, 5};
    force r = 8'hAA;
    release r;
    assign r = 8'h1;
    deassign r;
  end
endmodule
