// SV-THR-003: a new thread per item with nothing bounding them.
class bus_driver;
  mailbox #(int) req_mb = new();
  int unsigned handled;
  task handle_item(int tr);
    #10 handled++;
  endtask
  task run();
    int tr;
    forever begin
      req_mb.get(tr);
      fork // VIOLATION
        handle_item(tr);
      join_none
    end
  endtask
endclass
