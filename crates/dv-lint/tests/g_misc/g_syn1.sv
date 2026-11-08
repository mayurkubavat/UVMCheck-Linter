import "DPI-C" function int c_add(input integer a, int b);
import "DPI-C" function void c_put(reg x);
import "DPI-C" function void c_ok(int x);
module m;
  int q[$];
  int r;
  int d[];
  integer fd;
  initial begin
    r = q.find(x) with (x > 1)[0];
    r = q.find_first_index(x) with (x == 2)[0];
    r = (q.min())[0];
    for (int i = 0; i < 4; i++) begin
      d = new[4];
    end
    forever begin
      d = new[8](d);
      @(posedge r);
    end
    $fopen("a.txt");
    void'($fopen("b.txt"));
    fd = $fopen("c.txt", "w");
    $fdisplay(fd, "x");
  end
  task automatic t();
    integer f2;
    f2 = $fopen("d.txt");
    if (f2 == 0) $display("bad");
  endtask
  always @(posedge r) begin
    fork
      #1;
    join_none
  end
  always_ff @(posedge r) begin
    fork begin end join_any
  end
endmodule
