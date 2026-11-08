// Compile errors the native front end reports as slang does (SV-CMP-001).
// Each `// error:` comment names the message expected on that line.
interface bus_if (input logic clk);
  logic v;
  clocking cb @(posedge clk); output v; endclocking
  modport drv_mp (clocking cb);
endinterface

typedef struct packed { bit mml; bit mmwp; } cfg_t;

class item;
  rand cfg_t cfg;
  rand bit [3:0] x[4];
  rand int w;
  int plain;
  virtual bus_if vif;
  virtual bus_if.drv_mp mp;

  constraint order_c {
    solve plain before w;            // error: target of 'solve before' constraint must be a random variable
    solve cfg.mml before w;          // error: target of 'solve before' constraint must be a random variable
    foreach (x[i]) {
      solve w before x[i];           // error: constraint ordering directive is not allowed within a nested constraint block
    }
  }

  extern function void connect_mp();

  function void check_name(string s, string opts[$]);
    if (s[0] inside {opts}) w = 1;   // error: inside expression of type 'string' is not comparable to common type 'byte'
  endfunction
endclass

function void item::connect_mp();
  mp = vif.drv_mp;                   // error: 'drv_mp' cannot be used in an expression
endfunction

module sub; logic v; endmodule

module tb;
  sub u_sub();
  assign u_sub.u_missing.v = 1'b0;   // error: could not resolve hierarchical path name 'u_missing'
endmodule
