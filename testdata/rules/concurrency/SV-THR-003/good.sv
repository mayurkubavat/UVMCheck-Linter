// SV-THR-003 good: a semaphore bounds outstanding items; bounded loops are fine.
class bus_driver;
  mailbox #(int) req_mb = new();
  semaphore slots = new(4);
  int unsigned handled;
  task handle_item(int tr);
    #10 handled++;
    slots.put();
  endtask
  task run();
    int tr;
    forever begin
      req_mb.get(tr);
      slots.get();
      fork
        handle_item(tr);
      join_none
    end
  endtask
  task burst();
    for (int i = 0; i < 4; i++) begin
      fork
        handle_item(i);
      join_none
    end
    wait fork;
  endtask
endclass
