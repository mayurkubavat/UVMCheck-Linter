// Clause 13: tasks and functions. 13.3 tasks, 13.4 functions (void, automatic, static,
// return), 13.5 argument directions, ref and const ref, default and named arguments,
// 13.4.1 recursion, 13.5.4 argument binding by name.
module ch13_subroutines;
  task automatic wait_cycles(input int n = 1, output int waited);
    repeat (n) #1;
    waited = n;
  endtask
  function automatic int fact(int n);
    return n <= 1 ? 1 : n * fact(n - 1);
  endfunction
  function automatic void bump(ref int x, input int by = 1);
    x += by;
  endfunction
  function automatic int total(const ref int a [$]);
    int s = 0;
    foreach (a[i]) s += a[i];
    return s;
  endfunction
  function static int counter();
    static int c = 0;
    return ++c;
  endfunction
  initial begin
    int w, x = 1;
    int q [$] = {1, 2};
    wait_cycles(.n(2), .waited(w));
    bump(x);
    bump(.x(x), .by(3));
    $display("%0d %0d %0d %0d %0d", w, x, fact(4), total(q), counter());
  end
endmodule
