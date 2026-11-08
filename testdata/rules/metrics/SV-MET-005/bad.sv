// SV-MET-005: a condition with six logical operators.
class gate;
  bit valid, rst, err, en, busy, last, first;
  int cnt;
  function bit fire();
    if (valid && !rst && (busy || last) && cnt > 0 && !err && en && first) return 1; // VIOLATION
    return 0;
  endfunction
endclass
