// slang: --allow-genblk-reference
// A reference to an unnamed generate block by its implicit name genblk1 (27.6).
module probe;
  if (1) begin
    logic x;
  end
  initial genblk1.x = 1;
endmodule
