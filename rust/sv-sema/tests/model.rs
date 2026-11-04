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
