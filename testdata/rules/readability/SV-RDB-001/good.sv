// SV-RDB-001 good: named flags, a single flag, and int arguments.
class bus_driver;
  int unsigned sent;
  task send(int data, bit wait_rsp, bit lock, bit verbose);
    sent++;
  endtask
  task send_n(int count, int len);
    sent += count;
  endtask
  task enable(bit on);
    sent = 0;
  endtask
  task run();
    send(5, .wait_rsp(1), .lock(0), .verbose(1));
    send_n(1, 0);
    enable(1);
  endtask
endclass
