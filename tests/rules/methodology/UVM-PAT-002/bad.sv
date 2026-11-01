// UVM-PAT-002: a static variable of the class's own type is a hand-rolled singleton.
`include "uvm_mock.svh"

class report_svc;
  static report_svc inst; // VIOLATION
  static function report_svc get();
    if (inst == null) inst = new();
    return inst;
  endfunction : get
endclass : report_svc

class log_svc;
  static function log_svc get();
    static log_svc inst; // VIOLATION
    if (inst == null) inst = new();
    return inst;
  endfunction : get
endclass : log_svc
