typedef enum {READ, WRITE, IDLE} bus_kind_e;

// SV-DSN-009: the same enum is switched on in three classes.
class bus_driver;
  int unsigned n;
  function void drive(bus_kind_e k);
    case (k) // VIOLATION
      READ: n++;
      default: n--;
    endcase
  endfunction
endclass
class bus_monitor;
  int unsigned n;
  function void sample(bus_kind_e k);
    case (k) // VIOLATION
      WRITE: n++;
      default: n--;
    endcase
  endfunction
endclass
class bus_predictor;
  int unsigned n;
  function void predict(bus_kind_e k);
    case (k) // VIOLATION
      IDLE: n++;
      default: n--;
    endcase
  endfunction
endclass
