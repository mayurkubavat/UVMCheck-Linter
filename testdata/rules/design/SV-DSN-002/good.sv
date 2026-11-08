// SV-DSN-002 good: a small cohesive class.
class counter;
  int count;
  int limit;
  function void incr();
    if (count < limit) count++;
  endfunction
  function bit full();
    return count >= limit;
  endfunction
endclass
