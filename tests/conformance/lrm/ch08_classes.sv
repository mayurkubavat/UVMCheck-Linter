// Clause 8: classes. 8.7 constructors, 8.9-8.10 static members, 8.13-8.15 inheritance and super,
// 8.16 $cast, 8.18 local/protected, 8.19 const properties, 8.20-8.22 virtual and pure virtual,
// 8.23 class scope, 8.24 out-of-block declarations, 8.25 parameterized classes,
// 8.26 interface classes, 8.27 typedef class.
package ch08_pkg;
  typedef class fwd;
  interface class printable;
    pure virtual function string show();
  endclass
  virtual class shape implements printable;
    protected string nm;
    local int secret;
    const int sides;
    static int count;
    function new(string nm, int sides);
      this.nm = nm;
      this.sides = sides;
      count++;
    endfunction
    pure virtual function real area();
    virtual function string show();
      return $sformatf("%s %0d", nm, sides);
    endfunction
    extern function int get_secret();
    static function int how_many();
      return count;
    endfunction
  endclass
  function int shape::get_secret();
    return secret;
  endfunction
  class square extends shape;
    real side;
    function new(real side);
      super.new("square", 4);
      this.side = side;
    endfunction
    virtual function real area();
      return side * side;
    endfunction
  endclass
  class box #(type T = int, int N = 2);
    T items[N];
  endclass
  class fwd;
    box#(string, 3) b;
  endclass
endpackage

module ch08_classes;
  import ch08_pkg::*;
  initial begin
    shape s;
    square sq = new(2.0);
    square back;
    s = sq;
    if ($cast(back, s)) $display("%s %f %0d", s.show(), back.area(), shape::how_many());
  end
endmodule
