// Clause 6: data types. 6.5-6.8 nets and variables, 6.6.7 user-defined nettypes,
// 6.11-6.17 integer, real, void, chandle, class, string and event types, 6.18 typedef,
// 6.19 enums and their methods, 6.20 parameters and constants, 6.23 type operator,
// 6.24 casting, 6.25 parameterized data types.
package ch06_pkg;
  typedef enum logic [1:0] {IDLE, BUSY = 2'd2, DONE} state_e;
  typedef struct packed { logic [3:0] hi; logic [3:0] lo; } nib_t;
  class pcls #(type T = int);
    typedef T item_t[$];
  endclass
endpackage

module ch06_data_types import ch06_pkg::*; #(parameter int W = 8, parameter type DT = logic [W-1:0]);
  typedef logic [W-1:0] word_t;
  nettype logic [7:0] byte_net;
  byte_net bn;
  wire [W-1:0] w;
  tri0 t0;
  var logic v;
  DT dv;
  localparam int DEPTH = 2 ** 3;
  const int cval = 5;
  shortint si; longint li; byte b; bit [3:0] bt; integer ig;
  shortreal sr; realtime rt;
  chandle ch;
  string str;
  event ev;
  state_e st;
  nib_t nib;
  var type(nib) nib2;
  pcls#(word_t)::item_t q;
  assign w = '0;
  initial begin
    st = st.first();
    st = st.next();
    $display("%s %0d", st.name(), st.num());
    nib = nib_t'(8'hA5);
    nib2 = nib;
    li = longint'(si) + int'(3.7);
    v = $cast(st, 2'd2);
    q.push_back(word_t'(cval));
    str = $sformatf("%0d", $bits(word_t));
    -> ev;
    sr = 1.0; rt = 1.0; b = 0; bt = 0; ig = 0; ch = null; dv = '0;
  end
endmodule
