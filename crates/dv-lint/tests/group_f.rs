//! Style, readability and deprecated-API rules (builder slice N6.3,
//! group F).

use std::collections::HashMap;

fn counts(src: &str) -> HashMap<&'static str, usize> {
    let tree = svlang::syntax::parse_source(src, "t.sv", Default::default());
    let d = dv_model::build(std::slice::from_ref(&tree));
    let (_, found) = dv_lint::lint_all(&d, std::slice::from_ref(&tree), |_| true);
    let mut out = HashMap::new();
    for f in found {
        *out.entry(f.rule).or_default() += 1;
    }
    out
}

const MOCK: &str = "class uvm_object; function new(string name = \"\"); endfunction endclass\n\
    class uvm_component extends uvm_object; function new(string name, uvm_component parent); super.new(name); endfunction endclass\n\
    class uvm_driver extends uvm_component; function new(string name, uvm_component parent); super.new(name, parent); endfunction endclass\n";

#[test]
fn style_rules() {
    let src = format!(
        "{MOCK}typedef enum {{A, B}} state;\n\
         typedef struct {{ int a; }} pkt;\n\
         class my_drv extends uvm_driver;\n\
         function new(string name, uvm_component parent); super.new(name, parent); endfunction\n\
         task run(); int a; a = 1; a = 2;\n\
         if(a) a = 3;\n\
         endtask\n\
         endclass : other_name \n\
         // {}\n",
        "x".repeat(130)
    );
    let got = counts(&src);
    let n = |id: &str| got.get(id).copied().unwrap_or(0);
    assert_eq!(n("SV-STY-004"), 2, "{got:?}");
    assert_eq!(n("SV-STY-011"), 1, "{got:?}");
    assert_eq!(n("SV-STY-012"), 1, "{got:?}");
    assert_eq!(n("SV-STY-015"), 1, "{got:?}");
    // The two long mock lines and the comment.
    assert_eq!(n("SV-STY-016"), 3, "{got:?}");
    assert_eq!(n("SV-STY-006"), 1, "{got:?}");
}

#[test]
fn deprecated_rules() {
    let src = "module child; initial $display($random); endmodule\n\
               module top; child u1(); child u2(); endmodule\n";
    let got = counts(src);
    // Once, however many instances of `child` elaborate it.
    assert_eq!(got.get("SV-DEP-004"), Some(&1), "{got:?}");
}

#[test]
fn readability_rules() {
    let src = format!(
        "{MOCK}class tb_drv extends uvm_driver;\n\
         int tmp;\n\
         int cntrl_word;\n\
         function new(string name, uvm_component parent); super.new(name, parent); endfunction\n\
         virtual function void hook(); endfunction\n\
         endclass\n\
         class tb_drv2 extends tb_drv;\n\
         function new(string name, uvm_component parent); super.new(name, parent); endfunction\n\
         endclass\n"
    );
    let got = counts(&src);
    let n = |id: &str| got.get(id).copied().unwrap_or(0);
    assert_eq!(n("SV-RDB-010"), 2, "{got:?}");
    assert_eq!(n("SV-RDB-007"), 1, "{got:?}");
    assert_eq!(n("SV-RDB-008"), 1, "{got:?}");
    assert_eq!(n("SV-RDB-004"), 1, "{got:?}");
}
