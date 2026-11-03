// Clause 16: assertions. 16.3 immediate, 16.4 deferred, 16.7-16.9 sequences and operators,
// 16.10 local variables, 16.12 properties, 16.13 multiclock, 16.14 assert/assume/cover/
// restrict, 16.15 disable iff, 16.17 expect.
module ch16_assertions(input logic clk, input logic rst, input logic req, input logic gnt,
                       input logic [7:0] data);
  default clocking @(posedge clk); endclocking
  default disable iff (rst);
  sequence s_req_gnt(int max);
    req ##[1:max] gnt;
  endsequence
  property p_handshake;
    req |-> s_req_gnt(4);
  endproperty
  property p_data_stable;
    logic [7:0] v;
    (req, v = data) |=> (data == v) until_with gnt;
  endproperty
  a_hs: assert property (p_handshake) else $error("no grant");
  a_data: assert property (p_data_stable);
  c_req: cover property (req ##1 !req [*2] ##1 req);
  m_gnt: assume property (gnt |-> $past(req));
  restrict property (!(req && rst));
  a_mc: assert property (@(posedge clk) req |=> @(negedge clk) gnt);
  always_comb begin
    a_imm: assert (!(req && gnt && rst));
    a_def: assert final (data != 8'hFF);
  end
  initial begin
    expect (@(posedge clk) req ##1 gnt) else $display("expect failed");
  end
endmodule
