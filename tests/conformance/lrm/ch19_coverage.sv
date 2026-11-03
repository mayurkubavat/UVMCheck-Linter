// Clause 19: functional coverage. 19.3 covergroups with sampling event and arguments,
// 19.4 covergroups in classes, 19.5 coverpoints with bins, wildcard, illegal and ignore bins,
// transitions, with clauses, 19.6 crosses with binsof, 19.7 options, 19.8 sample and get_coverage.
class ch19_cov;
  bit [3:0] mode;
  bit [1:0] kind;
  covergroup cg with function sample(bit [3:0] m, bit [1:0] k);
    option.per_instance = 1;
    cp_mode: coverpoint m {
      bins low[] = {[0:3]};
      bins high = {[4:14]};
      wildcard bins odd = {4'b???1};
      illegal_bins bad = {15};
      bins seq = (0 => 1 => 2);
      bins evens = {[0:15]} with (item % 2 == 0);
    }
    cp_kind: coverpoint k { ignore_bins none = {3}; }
    x_mk: cross cp_mode, cp_kind {
      ignore_bins skip = binsof(cp_kind) intersect {0};
    }
  endgroup
  function new();
    cg = new();
  endfunction
  function void hit();
    cg.sample(mode, kind);
    $display("%f", cg.get_coverage());
  endfunction
endclass

module ch19_coverage(input logic clk, input logic [3:0] v);
  covergroup cg_mod @(posedge clk);
    coverpoint v;
  endgroup
  cg_mod inst = new();
endmodule
