// IEEE 1800-2023 additions (needs --std 1800-2023, which the lang-std override selects):
// final classes and :initial/:extends/:final method and constraint specifiers, real rand
// variables and constraints, dist default, solve-before on array size, covergroup extends,
// triple-quoted strings, array map, weak_reference, $timeunit, $stacktrace.
package ieee2023_pkg;
  class base;
    rand int x;
    rand real r;
    rand int q [];
    constraint c_r { r > 1.5; r < 9.0; }
    constraint c_d { x dist {0 := 1, [1:9] :/ 3, default :/ 1}; }
    constraint c_q { q.size() < 4; solve q.size() before x; }
    covergroup cg;
      coverpoint x;
    endgroup
    function new();
      cg = new();
    endfunction
    virtual function :initial void f();
    endfunction
  endclass
  class :final derived extends base;
    constraint :extends c_r { r < 5.0; }
    covergroup extends cg;
      coverpoint r { bins lo = {[0.0:2.0]}; }
    endgroup
    virtual function :extends :final void f();
    endfunction
  endclass
  function automatic void use_2023();
    int a [] = '{1, 2, 3};
    int b [];
    derived d = new();
    weak_reference #(derived) wr = new(d);
    string s = """two
lines""";
    b = a.map(e) with (e * 2);
    $display("%s %0d %t %t %s", s, b.size(), $timeunit, $timeprecision, $stacktrace);
    if (wr.get() != null) $display("alive");
  endfunction
endpackage
