use svlang::{DiagnosticStage, Driver};

#[test]
fn driver_compiles_multi_buffer_macros_and_elaborates() {
    let comp = Driver::new()
        .define("DATA_W", Some("32".into()))
        .add_source("defs.svh", "`define DECL_REG(name) logic [`DATA_W-1:0] name;\n")
        .add_source(
            "top.sv",
            "module leaf #(parameter int W = 8) (); endmodule\n\
             module top;\n\
               `DECL_REG(bus_a)\n\
               leaf #(.W(`DATA_W)) u_leaf ();\n\
             endmodule\n",
        )
        .top("top")
        .compile()
        .expect("in-memory compile succeeds");

    let (diags, elab) = comp.check();
    assert!(diags.is_empty(), "expected clean compile, got {diags:?}");
    assert_eq!(
        elab.hierarchy,
        ["$root instance top : top", "top instance u_leaf : leaf", "top.u_leaf parameter W = 32"]
    );
}

#[test]
fn diagnostics_report_semantic_errors_with_stage_and_location() {
    let comp = svlang::Compilation::from_source("bad.sv", "module bad;\n  initial x = 1;\nendmodule\n");
    let diags = comp.diagnostics();
    assert_eq!(diags.len(), 1);
    assert_eq!(diags[0].stage, DiagnosticStage::Semantic);
    assert_eq!(diags[0].file, "bad.sv");
    assert_eq!(diags[0].line, 2);
}
