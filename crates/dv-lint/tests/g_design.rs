//! Group D rules: design smells, metrics and dead code.

fn findings(src: &str) -> Vec<(u32, u32, &'static str)> {
    let tree = sv_syntax::parse_source(src, "t.sv", Default::default());
    let d = sv_sema::build(std::slice::from_ref(&tree));
    let (files, found) = dv_lint::lint_all(&d, std::slice::from_ref(&tree), |_| true);
    found
        .iter()
        .map(|f| {
            let (_, line, col) = f.position_in(&files);
            (line, col, f.rule)
        })
        .collect()
}

fn count(got: &[(u32, u32, &str)], line: u32, rule: &str) -> usize {
    got.iter().filter(|f| f.0 == line && f.2 == rule).count()
}

#[test]
fn dead_store_once_per_specialization_and_instance() {
    // The default specialization plus `gen #(byte)`.
    let src = "class gen #(type T = int);\n int x;\n task t();\n  x = 1;\n  x = 2;\n endtask\nendclass\n\
               class user; gen #(byte) g; gen #(int) h; endclass\n\
               module leaf; int z; initial begin\n z = 1;\n z = 2;\n end endmodule\n\
               module top; leaf u1(); leaf u2(); endmodule\n";
    let got = findings(src);
    assert_eq!(count(&got, 4, "SV-DED-004"), 2, "{got:?}");
    assert_eq!(count(&got, 10, "SV-DED-004"), 2, "{got:?}");
}

#[test]
fn dead_store_skips_bodies_slang_cannot_bind() {
    // `v += 1` on an enum is illegal: slang drops the whole block.
    let src = "module top;\n typedef enum {A, B} e_t;\n e_t v; int y;\n initial begin\n  v = A;\n  v += 1;\n  y = 1;\n  y += 2;\n end\n\
               initial begin\n  y = 1;\n  y += 2;\n end\nendmodule\n";
    let got = findings(src);
    assert_eq!(count(&got, 7, "SV-DED-004"), 0, "{got:?}");
    assert_eq!(count(&got, 11, "SV-DED-004"), 1, "{got:?}");
}

#[test]
fn override_only_calls_super_needs_a_known_base() {
    let src = "class base; virtual function void f(int a); endfunction endclass\n\
               class d1 extends base;\n function void f(int a); super.f(a); endfunction\nendclass\n\
               class d2 extends unknown_base;\n function void f(int a); super.f(a); endfunction\nendclass\n";
    let got = findings(src);
    assert_eq!(count(&got, 3, "SV-DSN-006"), 1, "{got:?}");
    assert_eq!(count(&got, 6, "SV-DSN-006"), 0, "{got:?}");
}

#[test]
fn metrics_rules_fire_on_complex_routines() {
    let mut body = String::new();
    for i in 0..30 {
        body.push_str(&format!(
            "  if (a > {i}) begin if (b) begin if (c) begin if (d) x = {i}; end end end\n"
        ));
    }
    let src = format!(
        "class m;\n int x; bit a, b, c, d;\n function void f();\n{body} endfunction\n\
         function void g(int a1, int a2, int a3, int a4, int a5, int a6, int a7, int a8); endfunction\nendclass\n"
    );
    let got = findings(&src);
    for rule in ["SV-MET-001", "SV-MET-002", "SV-MET-003", "SV-SML-004"] {
        assert!(got.iter().any(|f| f.2 == rule), "{rule} {got:?}");
    }
}
