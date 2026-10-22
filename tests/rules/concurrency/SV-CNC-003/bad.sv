// SV-CNC-003: busy-wait loops on a flag and an interface signal.
interface poll_if;
  logic ready;
endinterface

class bus_waiter;
  virtual poll_if vif;
  bit done;
  task wait_ready();
    while (!vif.ready) #1; // VIOLATION
  endtask
  task wait_done();
    forever begin // VIOLATION
      #10;
      if (done) break;
    end
  endtask
endclass
