//! `--explain RULE`: what a rule checks, the guidelines it enforces, the
//! options it reads, and its tested flagged and clean examples, in
//! uvmcheck's `--explain` layout. A sub-rule (a naming or style finding ID
//! such as UVM-NAM-101) is explained with the rule that reports it.

use crate::catalog;
use crate::guidelines::GUIDELINES;

include!(concat!(env!("OUT_DIR"), "/explain_data.rs"));

/// The explanation of `query` (an ID or name), with `bold` escape codes for a
/// terminal; Err for a rule nothing knows.
pub fn explain(query: &str, bold: bool) -> Result<String, String> {
    let all = catalog::entries();
    // A sub-rule is also known by the name its findings carry.
    let by_finding_name = || {
        crate::rules()
            .iter()
            .flat_map(|r| crate::sub_rules(r.id))
            .find(|s| s.finding_name == query)
            .and_then(|s| all.iter().find(|e| e.id == s.id))
    };
    let Some(e) = all.iter().find(|e| e.id == query || e.name == query).or_else(by_finding_name) else {
        return Err(format!("unknown rule '{query}' (see --list-rules)"));
    };
    let (id, name) = (e.id, e.name);
    let parent = e.reported_by.and_then(|p| all.iter().find(|x| x.id == p));
    let (b, r) = if bold { ("\x1b[1m", "\x1b[0m") } else { ("", "") };
    let mut out = format!(
        "{b}{id}  {name}{r}  [{}, {}{}{}]\n\n{}\n",
        e.category,
        e.severity.as_str(),
        if e.experimental { ", experimental: runs with --experimental" } else { "" },
        if e.native { ", native engine only" } else { "" },
        e.about
    );
    if let Some(p) = parent {
        out.push_str(&format!("\nReported by {} ({}).\n", p.id, p.name));
    }
    let mut guides = e.guidelines;
    if guides.is_empty() {
        if let Some(p) = parent {
            guides = p.guidelines;
        }
    }
    let lines: Vec<String> = guides
        .iter()
        .filter_map(|g| GUIDELINES.iter().find(|x| x.0 == *g))
        .map(|(gid, _, level, text)| format!("  {gid} ({level}) {text}\n"))
        .collect();
    if !lines.is_empty() {
        out.push_str("\nMethodology guidelines:\n");
        out.extend(lines);
    }
    // Options and examples belong to the finding ID, else to its rule.
    let owner = |pick: &dyn Fn(&str) -> bool| if pick(id) { id } else { parent.map_or(id, |p| p.id) };
    let opts_owner = owner(&|x| OPTIONS.iter().any(|o| o.0 == x));
    let opts: Vec<&(&str, &str, &str)> = OPTIONS.iter().filter(|o| o.0 == opts_owner).collect();
    if !opts.is_empty() {
        let class = parent.map_or(name, |p| p.name);
        out.push_str(&format!("\nOptions, set in .uvmcheck/config.py:\n    class {class}(RuleConfig):\n"));
        for (_, key, default) in opts {
            out.push_str(&format!("        {key} = {default}    # default\n"));
        }
    }
    let ex_owner = owner(&|x| EXAMPLES.iter().any(|e| e.0 == x));
    if let Some((_, dir, bad, good)) = EXAMPLES.iter().find(|e| e.0 == ex_owner) {
        show(&mut out, "Flagged", &format!("{dir}/bad.sv"), bad);
        show(&mut out, "Not flagged", &format!("{dir}/good.sv"), good);
    }
    out.push_str(&format!("\nWaive one finding with  // uvmcheck:waive {id} -- <reason>  on the flagged line.\n"));
    Ok(out)
}

fn show(out: &mut String, title: &str, path: &str, text: &str) {
    if text.is_empty() {
        return;
    }
    out.push_str(&format!("\n{title} ({path}):\n"));
    let mut lines = text.split('\n').peekable();
    let mut n = 0;
    while let Some(l) = lines.next() {
        // A final newline ends the text; it opens no empty line.
        if lines.peek().is_none() && l.is_empty() {
            break;
        }
        if n == 60 {
            out.push_str("    ...\n");
            break;
        }
        out.push_str(&format!("    {l}\n"));
        n += 1;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn explains_by_id_and_name() {
        let by_id = explain("SV-DUP-002", false).unwrap();
        assert!(by_id.starts_with("SV-DUP-002  CloneFragments  [Maintainability, WARNING]"));
        assert!(by_id.contains("MG-7.1 (Should)"));
        assert!(by_id.contains("min_tokens = 50    # default"));
        assert!(by_id.contains("Flagged (testdata/rules/methodology/SV-DUP-002/bad.sv):"));
        assert_eq!(explain("CloneFragments", false).unwrap(), by_id);
    }

    #[test]
    fn a_sub_rule_names_its_reporter() {
        let s = explain("UVM-NAM-101", false).unwrap();
        assert!(s.contains("Reported by SV-NAM-100 (ConfigurableNamingEngine)."));
    }

    #[test]
    fn an_unknown_rule_is_an_error() {
        assert!(explain("NO-SUCH-001", false).unwrap_err().contains("unknown rule"));
    }

    #[test]
    fn long_examples_are_cut_at_sixty_lines() {
        let mut out = String::new();
        let text: String = (0..70).map(|i| format!("l{i}\n")).collect();
        show(&mut out, "Flagged", "x", &text);
        assert_eq!(out.lines().count(), 1 + 1 + 60 + 1);
        assert!(out.ends_with("    l59\n    ...\n"));
    }
}
