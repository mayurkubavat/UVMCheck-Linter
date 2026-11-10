//! Two sessions with different settings lint at the same time in one
//! process, and neither sees the other's settings nor the process's.

const SRC: &str = "class c;\n\
                   function int add3(int a, int b, int d);\n\
                   return a + b + d;\n\
                   endfunction\n\
                   endclass\n";

/// How many findings SV-MET-003 (too many arguments) has in [`SRC`].
fn count(lint: impl FnOnce(&dv_model::Design, &[svlang::syntax::Tree]) -> Vec<dv_lint::Finding>) -> usize {
    let tree = svlang::syntax::parse_source(SRC, "t.sv", Default::default());
    let d = dv_model::build(std::slice::from_ref(&tree));
    lint(&d, std::slice::from_ref(&tree)).len()
}

#[test]
fn sessions_keep_their_own_settings_side_by_side() {
    let only = |id: &str| id == "SV-MET-003";
    let mut strict = dv_lint::Session::new();
    strict.set_options(&[("SV-MET-003".to_string(), "max_args".to_string(), 2)]);
    let plain = dv_lint::Session::new();
    std::thread::scope(|s| {
        let runs: Vec<_> = (0..8)
            .map(|i| {
                let (strict, plain) = (&strict, &plain);
                s.spawn(move || {
                    let session = if i % 2 == 0 { strict } else { plain };
                    (i % 2 == 0, count(|d, trees| session.lint_all(d, trees, only).1))
                })
            })
            .collect();
        for run in runs {
            let (is_strict, got) = run.join().expect("a lint run");
            assert_eq!(got, usize::from(is_strict));
        }
    });
    // The process's own session was not touched: three arguments are fine.
    assert_eq!(count(|d, trees| dv_lint::lint_all(d, trees, only).1), 0);
    // And it still is what the free setters change.
    dv_lint::options::set(&[("SV-MET-003".to_string(), "max_args".to_string(), 1)]);
    assert_eq!(count(|d, trees| dv_lint::lint_all(d, trees, only).1), 1);
    assert_eq!(count(|d, trees| plain.lint_all(d, trees, only).1), 0);
}
