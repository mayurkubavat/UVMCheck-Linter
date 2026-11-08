// SV-RDB-003: a block of statements left in comments.
class bus_driver;
  int unsigned sent;
  task drive();
    // sent++; // VIOLATION
    // #10;
    // sent--;
    sent = 0;
  endtask
endclass
