// Clause 9: processes. 9.2 initial/always/always_comb/always_ff/always_latch/final,
// 9.3 begin-end, fork-join (any, none), named blocks, labels, 9.4 delay and event control,
// iff, level-sensitive wait, 9.6 wait fork, disable, disable fork, 9.7 process class.
module ch09_processes(input logic clk, input logic rst_n, input logic en, input logic [3:0] d,
                      output logic [3:0] q, output logic [3:0] lat, output logic [3:0] comb);
  event go;
  always_ff @(posedge clk or negedge rst_n)
    if (!rst_n) q <= '0;
    else q <= d;
  always_latch if (en) lat <= d;
  always_comb comb = d ^ q;
  initial begin : main_blk
    process p;
    fork
      begin : a #1; -> go; end
      begin : b @(go); end
    join_any
    fork
      #5;
    join_none
    wait fork;
    fork begin p = process::self(); #10; end join_none
    #0 if (p != null) p.kill();
    wait (en == 1'b1);
    @(posedge clk iff en);
    disable fork;
    lbl: repeat (2) @(negedge clk);
  end
  final $display("done");
endmodule
