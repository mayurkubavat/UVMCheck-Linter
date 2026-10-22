// SV-BUG-005 good: convert an operand first, or keep the result integral.
class bw_stats;
  int unsigned bytes;
  int unsigned cycles;
  real util;
  int unsigned per_cycle;
  function void update();
    util = real'(bytes) / cycles;
    per_cycle = bytes / cycles;
  endfunction
endclass
