// Clause 7: aggregate data types. 7.2 structures, 7.3 unions (packed, tagged),
// 7.4 packed and unpacked arrays, 7.5 dynamic arrays, 7.8-7.9 associative arrays,
// 7.10 queues, 7.11 array querying, 7.12 array manipulation methods.
module ch07_aggregates;
  typedef struct { int id; string name; } rec_t;
  typedef union packed { logic [15:0] word; logic [1:0][7:0] bytes; } w_u;
  typedef union tagged { void Invalid; int Valid; } maybe_t;
  logic [3:0][7:0] packed2d;
  int unpacked [4][2];
  int dyn [];
  int assoc [string];
  int q [$];
  rec_t recs [$];
  w_u wu;
  maybe_t m;
  initial begin
    dyn = new[4];
    dyn = new[8](dyn);
    assoc["a"] = 1;
    if (assoc.exists("a")) assoc.delete("a");
    q = {3, 1, 2};
    q.push_front(0);
    void'(q.pop_back());
    q.sort();
    q.reverse();
    recs.push_back('{1, "x"});
    wu.word = 16'h1234;
    m = tagged Valid 5;
    packed2d[1] = wu.bytes[0];
    unpacked[0][1] = $size(dyn) + $dimensions(unpacked) + q.size() + recs[0].id;
    begin
      int found [$] = q.find with (item > 1);
      int idx [$] = q.find_index with (item == 2);
      int total = q.sum();
      $display("%0d %0d %0d %0d", found.size(), idx.size(), total, $high(unpacked));
    end
  end
endmodule
