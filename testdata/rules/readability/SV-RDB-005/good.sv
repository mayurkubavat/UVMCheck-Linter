// SV-RDB-005 good: direct boolean expressions and multi-bit compares.
class fifo_model;
  bit full;
  bit [1:0] mode;
  int q[$];
  function bit can_push();
    return !full;
  endfunction
  function bit not_empty();
    return q.size() > 0;
  endfunction
  function bit mode_one();
    return mode == 1;
  endfunction
endclass
