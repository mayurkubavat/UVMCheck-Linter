// SV-DED-013: arguments the body never uses.
class clk_utils;
  int unsigned cycles;
  task wait_clks(int n, bit verbose); // VIOLATION
    repeat (n) cycles++;
  endtask
endclass

function automatic int scale(int v, int factor); // VIOLATION
  return v * 2;
endfunction
