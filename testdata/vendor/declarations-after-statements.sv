// slang: none
// A declaration after a statement in a begin-end block (9.3.1: declarations come first).
module probe;
  initial begin
    $display("start");
    int x = 1;
    $display("%0d", x);
  end
endmodule
