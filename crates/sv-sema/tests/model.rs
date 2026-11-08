use dv_model::UvmKind;

fn design(src: &str) -> dv_model::Design {
    let tree = sv_syntax::parse_source(src, "t.sv", Default::default());
    sv_sema::build(&[tree])
}

#[test]
fn classes_bases_and_roles() {
    let d = design(
        "class uvm_object; endclass\nclass uvm_component extends uvm_object; endclass\n\
         class uvm_driver #(type T = int) extends uvm_component; endclass\n\
         class my_drv extends uvm_driver #(int); endclass\n",
    );
    let drv = d.find_class("my_drv").unwrap();
    assert_eq!(drv.base_name, "uvm_driver");
    assert_eq!(drv.uvm, UvmKind::Driver);
    assert!(d.find_class("uvm_driver").unwrap().is_parameterized);
}

#[test]
fn implements_is_not_a_base() {
    let d = design(
        "interface class printable; pure virtual function string show(); endclass\n\
         class shape implements printable; virtual function string show(); return \"\"; endfunction endclass\n",
    );
    let shape = d.find_class("shape").unwrap();
    assert_eq!(shape.base_name, "");
    let names: Vec<&str> = shape.methods.iter().map(|m| m.name.as_str()).collect();
    assert!(names.contains(&"rand_mode"));
    let printable = d.find_class("printable").unwrap();
    let names: Vec<&str> = printable.methods.iter().map(|m| m.name.as_str()).collect();
    assert!(names.contains(&"randomize") && !names.contains(&"rand_mode"));
}

#[test]
fn virtual_overrides_and_extern_bodies() {
    let d = design(
        "class a; virtual function void f(); endfunction endclass\n\
         class b extends a; function void f(); endfunction extern function void g(int x); endclass\n\
         function void b::g(int x); endfunction\n",
    );
    let b = d.find_class("b").unwrap();
    let f = b.methods.iter().find(|m| m.name == "f").unwrap();
    assert!(f.is_virtual && !f.declared_virtual);
    let g = b.methods.iter().find(|m| m.name == "g").unwrap();
    assert!(g.is_extern && g.returns_void);
    assert_eq!(g.arguments.len(), 1);
}

fn method<'d>(d: &'d dv_model::Design, class: &str, name: &str) -> &'d dv_model::Method {
    d.find_class(class).unwrap().methods.iter().find(|m| m.name == name).unwrap()
}

#[test]
fn body_calls_assignments_and_timing() {
    let d = design(
        "class base; function new(string n); endfunction virtual function void f(int x); endfunction endclass\n\
         class c extends base;\n\
           int q[$]; base h;\n\
           function new(string n); super.new(n); h = new(\"x\"); endfunction\n\
           task run(); #1 h.f(1); q.push_back(2); q[0] <= 3; @(q.size()); endtask\n\
         endclass\n",
    );
    let new = method(&d, "c", "new");
    assert!(new.calls_super_new);
    assert!(new.super_new.is_some_and(|(i, _)| i == 0));
    let calls: Vec<(&str, &str)> = new.calls.iter().map(|c| (c.name.as_str(), c.callee_scope.as_str())).collect();
    assert_eq!(calls, [("new", "base"), ("new", "base")]);
    assert_eq!(new.assignments[0].lhs, "h");
    assert_eq!(new.assignments[0].root_kind, dv_model::RootKind::Property);

    let run = method(&d, "c", "run");
    assert_eq!(run.timing_controls.len(), 2);
    let f = run.calls.iter().find(|c| c.name == "f").unwrap();
    assert!(f.callee_declared_virtual && f.receiver == "h");
    let push = run.calls.iter().find(|c| c.name == "push_back").unwrap();
    assert!(push.is_system && push.receiver == "q");
    assert!(run.assignments.iter().any(|a| a.is_non_blocking && a.root_name == "q"));
}

#[test]
fn bodies_slang_rejects_keep_only_timing() {
    let d = design(
        "class c;\n\
           missing_t m;\n\
           function void f(); m.go(); $display(\"x\"); endfunction\n\
           function void g(); #1 $display(\"y\"); endfunction\n\
           task t(); undeclared_task(); endtask\n\
         endclass\n",
    );
    assert!(method(&d, "c", "f").calls.is_empty());
    let g = method(&d, "c", "g");
    assert!(g.calls.is_empty() && g.timing_controls.len() == 1);
    assert!(method(&d, "c", "t").calls.is_empty());
}

#[test]
fn type_strings_as_slang_prints_them() {
    let d = design(
        "localparam int W = 8;\n\
         typedef enum logic [3:0] {P0, P1 = 4'hA, P2} pe_t;\n\
         package p; typedef logic [W-1:0] w_t; endpackage\n\
         class box #(type T = int, int N = 2); T t; endclass\n\
         class c;\n\
           int unsigned a; logic signed [W-1:0] b; int q[$]; int m[string]; bit [3:0] f[3];\n\
           p::w_t w; box #(string, 3) bx; box dflt; mailbox mb;\n\
           function pe_t g(pe_t e, p::w_t x); endfunction\n\
         endclass\n",
    );
    let c = d.find_class("c").unwrap();
    let ty = |n: &str| c.properties.iter().find(|p| p.name == n).unwrap().ty.clone();
    assert_eq!(ty("a"), "int unsigned");
    assert_eq!(ty("b"), "logic signed[7:0]");
    assert_eq!(ty("q"), "int$[$]");
    assert_eq!(ty("m"), "int$[string]");
    assert_eq!(ty("f"), "bit[3:0]$[0:2]");
    assert_eq!(ty("w"), "p::w_t");
    assert_eq!(ty("bx"), "box#(string,3)");
    assert_eq!(ty("dflt"), "box#(int,2)");
    assert_eq!(ty("mb"), "mailbox#(untyped)");
    let g = method(&d, "c", "g");
    assert_eq!(g.return_type, "pe_t");
    assert_eq!(g.arguments[0].canonical_type, "enum{P0=4'd0,P1=4'd10,P2=4'd11}pe_t");
    assert_eq!(g.arguments[1].canonical_type, "logic[7:0]");
    let bx = d.find_class("box").unwrap();
    assert_eq!(bx.properties[0].ty, "box::T");
}
