// Clauses 26-27: packages and generate. 26.2-26.4 packages, wildcard and explicit imports,
// imports in module headers, 26.6 export, 26.7 std package, 27.4 loop generate,
// 27.5 if and case generate, 27.6 unnamed generate blocks.
package ch26_base;
  typedef logic [7:0] byte_t;
  function automatic byte_t inc(byte_t v);
    return v + 1;
  endfunction
endpackage

package ch26_mid;
  import ch26_base::*;
  export ch26_base::byte_t;
  localparam byte_t ZERO = '0;
endpackage

module ch26_27_packages_generate import ch26_mid::*; #(parameter int N = 3, parameter int MODE = 1);
  byte_t regs [N];
  for (genvar i = 0; i < N; i++) begin : g_regs
    initial regs[i] = ch26_base::inc(ZERO + byte_t'(i));
  end
  if (MODE == 0) begin : g_mode0
    initial $display("mode0");
  end else begin : g_other
    initial $display("other");
  end
  case (N)
    1: begin : g_one end
    default: begin : g_many end
  endcase
  initial begin
    std::semaphore s = new();
    $display("%0d", g_regs[0].i);
  end
endmodule
