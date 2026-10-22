// SV-DSN-007 good: an empty hook in the base may stay empty; overrides that do work are fine.
class bus_monitor;
  int unsigned seen;
  virtual function void pre_collect(int v);
  endfunction
  virtual function void collect(int v);
    pre_collect(v);
    seen++;
  endfunction
endclass

class quiet_monitor extends bus_monitor;
  int unsigned skipped;
  virtual function void pre_collect(int v);
  endfunction
  virtual function void collect(int v);
    skipped++;
  endfunction
endclass
