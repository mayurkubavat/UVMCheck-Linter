// Clause 18: constrained random. 18.4 rand/randc, 18.5 constraint blocks (inside, dist,
// implication, if-else, foreach, solve-before, soft, static, unique), 18.6 randomize and
// pre/post_randomize, 18.7 inline constraints with local::, 18.8-18.9 rand_mode and
// constraint_mode, 18.11 randomize(args), 18.12 std::randomize, 18.13 $urandom,
// 18.16 randcase, 18.17 randsequence.
class ch18_item;
  rand bit [7:0] addr;
  randc bit [3:0] id;
  rand int len;
  rand int data [];
  rand bit wr;
  rand bit [7:0] alt;
  int limit = 16;
  constraint c_addr { addr inside {[8'h10:8'h1F], 8'hFF}; }
  constraint c_len { len dist {1 := 5, [2:4] :/ 3}; soft len < 8; len > 0; }
  constraint c_data { data.size() == len; foreach (data[i]) data[i] < 100; solve len before data; }
  constraint c_wr { wr -> addr != 8'hFF; if (!wr) len == 1; else len <= limit; }
  constraint c_unique { unique {addr, alt}; }
  function void pre_randomize(); endfunction
  function void post_randomize(); endfunction
endclass

module ch18_random;
  initial begin
    ch18_item it = new();
    int limit = 4;
    int x;
    void'(it.randomize() with { len <= local::limit; });
    it.c_unique.constraint_mode(0);
    it.wr.rand_mode(0);
    void'(it.randomize(addr));
    void'(std::randomize(x) with { x inside {[0:9]}; });
    x = $urandom_range(10, 1) + $urandom();
    randcase
      3: x = 1;
      1: x = 2;
    endcase
    randsequence (main)
      main : first second;
      first : { x++; };
      second : { x--; } | { x += 2; };
    endsequence
  end
endmodule
