// SV-MET-005 good: parts named.
class gate;
  bit valid, rst, err, en, busy, last;
  int cnt;
  function bit fire();
    bit active = valid && !rst && en;
    bit pending = (busy || last) && cnt > 0;
    return active && pending && !err;
  endfunction
endclass
