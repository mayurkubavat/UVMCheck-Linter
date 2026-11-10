//! The catalog the rules declare: complete, consistent, and the one
//! docs/rules/catalog.json publishes.

use dv_lint::catalog;

#[test]
fn every_rule_is_fully_declared() {
    let all = catalog::entries();
    let guide: Vec<&str> = catalog::guidelines().iter().map(|g| g.id).collect();
    for pair in all.windows(2) {
        assert_ne!(pair[0].id, pair[1].id, "two entries share an id");
    }
    for e in &all {
        assert!(catalog::CATEGORIES.contains(&e.category), "{}: unknown category {}", e.id, e.category);
        assert!(e.profiles != 0, "{}: no profile, so no preset selects it", e.id);
        assert!(e.about.len() > 20 && e.about.ends_with('.'), "{}: description is not a sentence", e.id);
        assert!(!e.name.is_empty() && e.name.chars().all(|c| c.is_ascii_alphanumeric()), "{}: name {}", e.id, e.name);
        for g in e.guidelines {
            assert!(guide.contains(g), "{}: guideline {g} is not in docs/methodology", e.id);
        }
    }
}

#[test]
fn published_catalog_is_current() {
    let file = concat!(env!("CARGO_MANIFEST_DIR"), "/../../docs/rules/catalog.json");
    let published = std::fs::read_to_string(file).unwrap();
    assert!(published == catalog::rules_json(), "docs/rules/catalog.json is stale: run tools/codegen/gen_rule_docs.py");
}

#[test]
fn presets_follow_the_declared_profiles() {
    let all = dv_lint::rules();
    let style = dv_lint::rule_selection(Some("style"), false).unwrap();
    let default = dv_lint::rule_selection(None, false).unwrap();
    let with_experimental = dv_lint::rule_selection(Some("all"), true).unwrap();
    for r in &all {
        assert_eq!(style(r.id), !r.experimental && r.profiles & dv_lint::profile::STYLE != 0, "{}", r.id);
        assert_eq!(default(r.id), !r.experimental && r.profiles & dv_lint::DEFAULT_PROFILES != 0, "{}", r.id);
        assert!(with_experimental(r.id), "{}", r.id);
    }
    // Not a built-in rule: a Python rule always runs.
    assert!(style("MY-RULE-001"));
}
