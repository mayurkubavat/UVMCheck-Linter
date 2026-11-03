// Clauses 20-21: utility and I/O system tasks. 20.2 $finish/$stop, 20.3 time functions,
// 20.4 $printtimescale, 20.5 conversions, 20.6 $bits/$typename, 20.8 math, 20.9 bit-vector
// functions, 20.10 severity tasks, 20.11 assertion control, 20.12 sampled values,
// 20.14 distribution functions, 21.2 display, 21.3 file I/O, 21.4 $readmemh, 21.6 plusargs,
// 21.7 VCD.
module ch20_21_systasks(input logic clk, input logic sig);
  logic [7:0] mem [16];
  int fd;
  int seed = 1;
  string arg;
  always @(posedge clk) if ($rose(sig) || $fell(sig) || $stable(sig) || $changed(sig)) $display("%b", $past(sig));
  initial begin
    $printtimescale;
    $display("%t %0d %f", $time, $stime, $realtime);
    $display("%0d %f %0d", $rtoi(2.5), $itor(3), $signed(8'hFF));
    $display("%0d %s", $bits(mem), $typename(mem));
    $display("%f %0d", $sqrt(2.0), $clog2(17));
    $display("%0d %0d %0d", $countones(8'h0F), $onehot(4'b0100), $isunknown(sig));
    $display("%0d", $dist_uniform(seed, 0, 10));
    $assertoff(0);
    $asserton(0);
    fd = $fopen("out.txt", "w");
    $fwrite(fd, "%0d\n", 1);
    $fclose(fd);
    $readmemh("mem.hex", mem);
    if ($value$plusargs("ARG=%s", arg) || $test$plusargs("FLAG")) $info("arg %s", arg);
    $dumpfile("w.vcd");
    $dumpvars(0, ch20_21_systasks);
    $warning("w");
    $error("e");
    $finish(0);
  end
endmodule
