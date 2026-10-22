// SV-BUG-005: integer division truncates before the result becomes real.
class bw_stats;
  int unsigned bytes;
  int unsigned cycles;
  real util;
  function void update();
    util = bytes / cycles; // VIOLATION
  endfunction
  function real ratio(int a, int b);
    return a / b; // VIOLATION
  endfunction
endclass
