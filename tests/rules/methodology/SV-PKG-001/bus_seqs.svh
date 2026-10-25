// SV-PKG-001 helper: more than one class in one file.
class bus_rd_seq;
  int addr;
endclass

class bus_wr_seq; // VIOLATION
  int addr;
endclass
