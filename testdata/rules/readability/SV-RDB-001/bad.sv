// SV-RDB-001: a call with three positional 0/1 flags.
class bus_driver;
  int unsigned sent;
  task send(int data, bit wait_rsp, bit lock, bit verbose);
    sent++;
  endtask
  task run();
    send(5, 1, 0, 1); // VIOLATION
  endtask
endclass
