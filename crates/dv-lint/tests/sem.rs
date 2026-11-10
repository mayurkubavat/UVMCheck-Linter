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
        svlang::syntax::parse_source(LIB, "lib.sv", Default::default()),
        svlang::syntax::parse_source(TB, "tb.sv", Default::default()),
    ];
    let d = dv_model::build(&trees);
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
    let trees = [svlang::syntax::parse_source(src, "t.sv", Default::default())];
    let d = dv_model::build(&trees);
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

#[test]
fn a_monitor_drives_an_interface_by_declaration() {
    let src = "\
interface bus_if(input logic clk);
    logic valid;
endinterface
typedef struct { int n; } intf_stats_t;
class uvm_monitor; endclass
class my_monitor extends uvm_monitor;
    virtual bus_if bus;
    intf_stats_t intf_stats;
    task run();
        bus.valid = 1;
        intf_stats.n = 2;
        bus = null;
    endtask
endclass
";
    // The handle is `bus`, not `vif`; the struct only has `intf` in its name.
    assert_eq!(lint(src, &["UVM-VIF-DIR"]), [("UVM-VIF-DIR", 10)]);
}

/// A UVM small enough to read: the classes the objection rules look for.
const MINI_UVM: &str = "\
class uvm_object; endclass
class uvm_objection;
    function void raise_objection(uvm_object o = null, string d = \"\"); endfunction
    function void drop_objection(uvm_object o = null, string d = \"\"); endfunction
endclass
class uvm_phase extends uvm_objection; endclass
class uvm_component extends uvm_object;
    virtual task run_phase(uvm_phase phase); endtask
endclass
class uvm_test extends uvm_component; endclass
class uvm_sequence_base extends uvm_object;
    uvm_phase starting_phase;
endclass
";

const OBJECTION_RULES: [&str; 3] = ["UVM-OBJ-001", "UVM-OBJ-003", "UVM-PHS-006"];

/// Findings of the three objection pairing rules in `tb` (line 1 is its
/// first line), compiled after [`MINI_UVM`].
fn objections(tb: &str) -> Vec<(&'static str, u32)> {
    let base = MINI_UVM.lines().count() as u32;
    lint(&format!("{MINI_UVM}{tb}"), &OBJECTION_RULES).into_iter().map(|(r, l)| (r, l - base)).collect()
}

#[test]
fn an_objection_dropped_in_another_routine_of_the_class_is_paired() {
    // The UVM 1.1 sequence idiom, and a pair of helper tasks in a base class.
    let tb = "\
class seq_c extends uvm_sequence_base;
    task pre_body();
        if (starting_phase != null) starting_phase.raise_objection(this);
    endtask
    task post_body();
        if (starting_phase != null) starting_phase.drop_objection(this);
    endtask
endclass
class base_test extends uvm_test;
    task finish(uvm_phase phase);
        phase.drop_objection(this);
    endtask
endclass
class my_test extends base_test;
    task run_phase(uvm_phase phase);
        phase.raise_objection(this);
        if (phase == null) return;
        #10;
    endtask
endclass
";
    assert_eq!(objections(tb), []);
}

#[test]
fn an_objection_nobody_drops_is_still_reported() {
    let tb = "\
class my_test extends uvm_test;
    task run_phase(uvm_phase phase);
        phase.raise_objection(this);
        #10;
    endtask
    task main_phase(uvm_phase phase);
        phase.raise_objection(this);
        #10;
        phase.drop_objection(this);
    endtask
endclass
class late_test extends uvm_test;
    task run_phase(uvm_phase phase);
        #5 phase.drop_objection(this);
    endtask
endclass
";
    // main_phase is balanced in itself: it does not drop run_phase's.
    assert_eq!(
        objections(tb),
        [("UVM-OBJ-001", 3), ("UVM-OBJ-001", 14), ("UVM-OBJ-003", 2), ("UVM-OBJ-003", 14), ("UVM-PHS-006", 3)]
    );
}

#[test]
fn a_raise_and_a_drop_under_the_same_condition_balance() {
    let tb = "\
class my_test extends uvm_test;
    bit quick;
    task run_phase(uvm_phase phase);
        if (!quick) phase.raise_objection(this);
        #10;
        if (!quick) phase.drop_objection(this);
    endtask
    task main_phase(uvm_phase phase);
        phase.raise_objection(this);
        if (quick) phase.drop_objection(this);
        else #10;
    endtask
endclass
";
    // Dropped in one branch only is still a defect.
    assert_eq!(objections(tb), [("UVM-OBJ-003", 10)]);
}

#[test]
fn each_objection_handle_is_followed_on_its_own() {
    let tb = "\
class my_test extends uvm_test;
    uvm_objection done;
    task run_phase(uvm_phase phase);
        phase.raise_objection(this);
        done.raise_objection(this);
        #10;
        done.drop_objection(this);
        phase.drop_objection(this);
        done.drop_objection(this);
    endtask
endclass
";
    // The second drop of `done` has nothing to drop; `phase` is balanced.
    assert_eq!(objections(tb), [("UVM-OBJ-003", 9)]);
}

#[test]
fn a_return_inside_a_loop_while_the_objection_is_held_is_found() {
    let tb = "\
class my_test extends uvm_test;
    int q[$];
    task run_phase(uvm_phase phase);
        phase.raise_objection(this);
        foreach (q[i]) begin
            if (q[i] < 0) return;
        end
        forever begin
            #1;
            if (q.size() == 0) break;
        end
        phase.drop_objection(this);
    endtask
    task main_phase(uvm_phase phase);
        forever begin
            phase.raise_objection(this);
            #10;
            phase.drop_objection(this);
        end
    endtask
endclass
";
    // The loop in main_phase is balanced in each round.
    assert_eq!(objections(tb), [("UVM-OBJ-003", 6)]);
}
