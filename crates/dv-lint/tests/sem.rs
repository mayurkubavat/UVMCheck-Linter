//! Rules that ask what a reference reaches (`sem::reach`) answer from the
//! declarations, not from how the handle is named: SV-INT-001 and
//! SV-LANG-005 on interface and clocking block drives.

/// The interface, the config and the base class: another file than the driver's.
const LIB: &str = "\
typedef virtual bus_if bus_vif_t;
interface bus_if(input logic clk);
    logic [7:0] data;
    clocking drv @(posedge clk);
        output data;
    endclocking
    modport mp(clocking drv);
    task automatic park();
        data = 0;
    endtask
endinterface
typedef struct { int x; } knobs_t;
class cfg_c;
    virtual bus_if.mp bus;
    bus_vif_t alt;
endclass
class base_c;
    cfg_c cfg;
    virtual bus_if port[2];
endclass
class cbs_c;
    string fname;
endclass
";

const TB: &str = "\
class drv_c #(type V = virtual bus_if) extends base_c;
    knobs_t my_vif;
    V vif;
    task run();
        cfg.bus.drv.data = 1;
        cfg.alt.data = 2;
        this.port[1].data = 3;
        my_vif.x = 4;
        vif.data = 5;
        cfg.bus.drv.data <= 6;
        for (cbs_c cb = null; cb != null; ) cb.fname = \"x\";
        cfg.alt = null;
    endtask
    function void f();
        cfg.bus.drv.data <= 1;
        my_vif.x <= 2;
    endfunction
endclass
module top;
    logic clk;
    bus_if u_bus(clk);
    initial u_bus.data = 0;
    initial u_bus.drv.data = 0;
endmodule
";

/// (rule, line in tb.sv) of each finding of the two rules.
fn findings() -> Vec<(&'static str, u32)> {
    let trees = [
        sv_syntax::parse_source(LIB, "lib.sv", Default::default()),
        sv_syntax::parse_source(TB, "tb.sv", Default::default()),
    ];
    let d = sv_sema::build(&trees);
    let only = |id: &str| matches!(id, "SV-INT-001" | "SV-LANG-005");
    let mut out: Vec<(&'static str, u32)> = dv_lint::Session::new()
        .lint_all(&d, &trees, only)
        .1
        .iter()
        .map(|f| {
            let (file, line, _) = f.position(&d);
            assert!(file.ends_with("tb.sv"), "{} reported in {file}", f.rule);
            (f.rule, line)
        })
        .collect();
    out.sort();
    out
}

#[test]
fn interface_drives_are_recognised_by_declaration() {
    assert_eq!(
        findings(),
        [
            // `bus` is a virtual interface and `drv` its clocking block.
            ("SV-INT-001", 5),
            // `alt` is a typedef of a virtual interface.
            ("SV-INT-001", 6),
            // `port` is inherited from a class in another file.
            ("SV-INT-001", 7),
            // `V` is a type parameter: not resolved, so judged by the name `vif`.
            ("SV-INT-001", 9),
            // A clocking block of an interface instance.
            ("SV-INT-001", 23),
            // `my_vif` is a struct, whatever its name: '<=' in a function.
            ("SV-LANG-005", 16),
        ]
    );
}

/// (rule, line) of the findings of `rules` in `src`.
fn lint(src: &str, rules: &[&str]) -> Vec<(&'static str, u32)> {
    let trees = [sv_syntax::parse_source(src, "t.sv", Default::default())];
    let d = sv_sema::build(&trees);
    let mut out: Vec<(&'static str, u32)> = dv_lint::Session::new()
        .lint_all(&d, &trees, |id: &str| rules.contains(&id))
        .1
        .iter()
        .map(|f| (f.rule, f.position(&d).1))
        .collect();
    out.sort();
    out
}

#[test]
fn an_interface_is_one_by_declaration_not_by_suffix() {
    let src = "\
interface apb_bus;
    logic psel;
endinterface
interface class printable_if;
    pure virtual function void print();
endclass
class env_c;
    apb_bus bus;
    printable_if helper;
    unknown_if other;
endclass
";
    // `apb_bus` is an interface without the suffix; `printable_if` is a
    // class with it; `unknown_if` is not declared, so its name decides.
    assert_eq!(lint(src, &["SV-OOP-008"]), [("SV-OOP-008", 8), ("SV-OOP-008", 10)]);
}

#[test]
fn a_task_drives_an_interface_by_declaration() {
    let src = "\
interface apb_bus(input logic clk);
    logic psel;
    clocking drv @(posedge clk);
        output psel;
    endclocking
endinterface
typedef struct { int level; } vif_stats_t;
class drv_c;
    virtual apb_bus bus;
    vif_stats_t vif_stats;
    int count;
    bit in_reset;
    task send();
        bus.drv.psel <= 1;
    endtask
    task tally();
        vif_stats.level <= 1;
        count <= count + 1;
    endtask
    task guarded();
        wait (!in_reset);
        bus.drv.psel <= 1;
    endtask
endclass
";
    assert_eq!(lint(src, &["SV-DRV-001"]), [("SV-DRV-001", 13)]);
}
