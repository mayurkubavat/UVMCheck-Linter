//! The UVM API packs are chosen by the compiled UVM's version string, and
//! the 1.x packs drive UVM-API-002/003 there.

use std::fs;

/// A UVM 1.2-like library: its version string comes from macros, as the
/// real one's does.
const UVM12: &str = "`define UVM_NAME UVM\n`define UVM_MAJOR_REV 1\n`define UVM_MINOR_REV 2\n\
`define UVM_VERSION_STRING `\"`UVM_NAME``-```UVM_MAJOR_REV``.```UVM_MINOR_REV```\"\n\
package uvm_pkg;\n\
parameter string uvm_revision = `UVM_VERSION_STRING;\n\
class uvm_object; function new(string name = \"\"); endfunction endclass\n\
class uvm_component extends uvm_object;\n\
  function new(string name, uvm_component parent); super.new(name); endfunction\n\
  virtual function void set_config_int(string i, string f, int v); endfunction\n\
  function void print_config(); endfunction\n\
endclass\n\
class uvm_sequence_base extends uvm_object; function new(string name = \"\"); super.new(name); endfunction endclass\n\
class uvm_random_sequence extends uvm_sequence_base; function new(string name = \"\"); super.new(name); endfunction endclass\n\
endpackage\n";

const USER: &str = "import uvm_pkg::*;\n\
class my_rand extends uvm_random_sequence;\n\
  function new(string name = \"r\"); super.new(name); endfunction\n\
endclass\n\
class my_comp extends uvm_component;\n\
  `uvm_sequencer_utils(my_comp)\n\
  function new(string name, uvm_component parent); super.new(name, parent); endfunction\n\
  function void build(); set_config_int(\"*\", \"n\", 3); print_config(); endfunction\n\
endclass\n";

fn run(uvm: &str, ids: &[&str]) -> Vec<String> {
    let d = std::env::temp_dir().join(format!("dv-lint-uvm-api-{}-{}", std::process::id(), ids.join("_")));
    fs::create_dir_all(&d).unwrap();
    fs::write(d.join("uvm_pkg.sv"), uvm).unwrap();
    fs::write(d.join("user.sv"), USER).unwrap();
    let args = [
        "--single-unit".to_string(),
        d.join("uvm_pkg.sv").display().to_string(),
        d.join("user.sv").display().to_string(),
    ];
    let (design, trees) = sv_sema::load_with_trees(&args).unwrap();
    let (files, found) = dv_lint::lint_all(&design, &trees, |r| ids.contains(&r));
    fs::remove_dir_all(&d).ok();
    found
        .iter()
        .map(|f| {
            let (_, line, _) = f.position_in(&files);
            format!("{line} {} {}", f.rule, f.message)
        })
        .collect()
}

#[test]
fn uvm_1_2_gets_its_own_pack() {
    let got = run(UVM12, &["UVM-API-001", "UVM-API-002", "UVM-API-003"]);
    // print_config is Accellera-only in UVM 2020; UVM 1.2 has no IEEE tags.
    assert!(!got.iter().any(|l| l.contains("UVM-API-001")), "{got:?}");
    let api2: Vec<_> = got.iter().filter(|l| l.contains("UVM-API-002")).collect();
    assert_eq!(api2.len(), 1, "{got:?}");
    assert!(api2[0].starts_with("6 ") && api2[0].contains("`uvm_sequencer_utils is deprecated in UVM 1.2"), "{got:?}");
    assert!(api2[0].contains("UVM_NO_DEPRECATED"), "{got:?}");
    let api3: Vec<_> = got.iter().filter(|l| l.contains("UVM-API-003")).collect();
    assert_eq!(api3.len(), 2, "{got:?}");
    assert!(api3[0].starts_with("2 ") && api3[0].contains("class 'uvm_random_sequence' ('my_rand' extends it)"), "{got:?}");
    assert!(api3[1].starts_with("8 ") && api3[1].contains("'uvm_component::set_config_int' (called here)"), "{got:?}");
}

#[test]
fn uvm_2020_keeps_the_2020_pack() {
    let uvm = UVM12.replace("parameter string uvm_revision = `UVM_VERSION_STRING;", "parameter string UVM_VERSION_STRING = \"Accellera:1800.2:UVM:2020.3.1\";");
    let got = run(&uvm, &["UVM-API-001", "UVM-API-003"]);
    assert!(got.iter().any(|l| l.contains("UVM-API-001") && l.contains("print_config") && l.contains("UVM 2020.3.1")), "{got:?}");
    assert!(!got.iter().any(|l| l.contains("UVM-API-003")), "{got:?}");
}
