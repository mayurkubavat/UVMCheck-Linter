// A static counter and a static handle of another type are not a singleton of this class.
`include "uvm_mock.svh"

class bus_cfg;
  int addr;
  function void set_addr(int v);
    addr = v;
  endfunction : set_addr
  function int get_addr();
    return addr;
  endfunction : get_addr
endclass : bus_cfg

class report_svc;
  int id;
  static int live_count;
  bus_cfg cfg;
  static bus_cfg shared_cfg;
  static report_svc items[$] = {};

  function void set_id(int v);
    id = v;
    live_count = v;
  endfunction : set_id

  function void set_cfg(int a);
    cfg = new();
    shared_cfg = new();
    cfg.set_addr(a);
  endfunction : set_cfg

  function int summarize();
    return id + live_count + cfg.get_addr() + shared_cfg.get_addr() + items.size();
  endfunction : summarize
endclass : report_svc
