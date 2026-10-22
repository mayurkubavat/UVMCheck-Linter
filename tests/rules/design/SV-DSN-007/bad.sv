// SV-DSN-007: a subclass switches off the base monitor's collection.
class bus_monitor;
  int unsigned seen;
  virtual function void collect(int v);
    seen++;
  endfunction
endclass

class quiet_monitor extends bus_monitor;
  virtual function void collect(int v); // VIOLATION
  endfunction
endclass
