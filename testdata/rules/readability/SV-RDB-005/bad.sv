// SV-RDB-005: boolean code that can be simplified.
class fifo_model;
  bit full;
  int q[$];
  function bit can_push();
    if (full == 1'b1) return 0; // VIOLATION
    return 1;
  endfunction
  function bit not_empty();
    if (q.size() > 0) return 1; // VIOLATION
    else return 0;
  endfunction
endclass
