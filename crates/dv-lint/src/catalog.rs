//! The rule catalog: every rule and sub-rule with its category, severity,
//! profiles, guidelines and description, read from the rules' own
//! declarations ([`crate::Rule`], [`crate::Sub`]). `--list-rules`,
//! `--list-guidelines`, `--explain`, the report rule tables and
//! docs/rules/ (tools/codegen/gen_rule_docs.py) all come from here.

use crate::guidelines::GUIDELINES;
use crate::json::Json;
use crate::{profile, Severity};

/// The categories, in the order the catalog lists them.
pub const CATEGORIES: [&str; 20] = [
    "UVM",
    "OOP",
    "Architecture",
    "Correctness",
    "Concurrency",
    "Randomization",
    "Coverage",
    "SVA",
    "Types",
    "DeadCode",
    "Performance",
    "Portability",
    "Security",
    "DPI",
    "RTL_TB_Bridge",
    "Deprecated",
    "Style",
    "Naming",
    "Maintainability",
    "Advisory",
];

/// One catalog line: a rule, or an id a rule reports under.
#[derive(Clone, Debug)]
pub struct Entry {
    /// Finding id.
    pub id: &'static str,
    /// Catalog name.
    pub name: &'static str,
    /// Category, one of [`CATEGORIES`].
    pub category: &'static str,
    /// Default severity.
    pub severity: Severity,
    /// [`profile`] bits.
    pub profiles: u32,
    /// Methodology guidelines enforced.
    pub guidelines: &'static [&'static str],
    /// Runs only with `--experimental`.
    pub experimental: bool,
    /// Only the native engine has it.
    pub native: bool,
    /// The rule that reports this id, for a sub-rule.
    pub reported_by: Option<&'static str>,
    /// What it flags.
    pub about: &'static str,
}

impl Entry {
    /// The profile names (`["soundness", "security"]`).
    pub fn profile_names(&self) -> Vec<&'static str> {
        profile::NAMES.iter().enumerate().filter(|(i, _)| self.profiles & (1 << i) != 0).map(|(_, n)| *n).collect()
    }
}

/// Every rule and sub-rule, by id.
pub fn entries() -> Vec<Entry> {
    let mut out = Vec::new();
    for r in crate::rules() {
        out.push(Entry {
            id: r.id,
            name: r.name,
            category: r.category,
            severity: r.severity,
            profiles: r.profiles,
            guidelines: r.guidelines,
            experimental: r.experimental,
            native: r.native,
            reported_by: None,
            about: r.about,
        });
        for s in crate::sub_rules(r.id) {
            out.push(Entry {
                id: s.id,
                name: s.name,
                category: r.category,
                severity: s.severity,
                profiles: r.profiles,
                guidelines: &[],
                experimental: r.experimental,
                native: r.native,
                reported_by: Some(r.id),
                about: s.about,
            });
        }
    }
    out.sort_by_key(|e| e.id);
    out
}

/// The entry of a rule or sub-rule, by id or name.
pub fn find(query: &str) -> Option<Entry> {
    entries().into_iter().find(|e| e.id == query || e.name == query)
}

fn q(s: &str) -> String {
    Json::Str(s.to_string()).to_string()
}

fn list(items: &[&str]) -> String {
    format!("[{}]", items.iter().map(|s| q(s)).collect::<Vec<_>>().join(", "))
}

/// The catalog as JSON (`--list-rules --json`): docs/rules/catalog.json.
pub fn rules_json() -> String {
    let mut o = String::from("{\n  \"version\": \"2.0\",\n  \"generated\": \"uvmcheck --list-rules --json\",\n");
    o.push_str(&format!("  \"categories\": {},\n  \"rules\": [\n", list(&CATEGORIES)));
    let all = entries();
    for (i, e) in all.iter().enumerate() {
        o.push_str(&format!(
            "    {{\"id\": {}, \"name\": {}, \"category\": {}, \"severity\": {}, \"profiles\": {}, \"guidelines\": {},",
            q(e.id),
            q(e.name),
            q(e.category),
            q(e.severity.as_str()),
            list(&e.profile_names()),
            list(e.guidelines)
        ));
        if e.experimental {
            o.push_str(" \"maturity\": \"experimental\",");
        }
        if let Some(p) = e.reported_by {
            o.push_str(&format!(" \"reportedBy\": {},", q(p)));
        }
        if e.native {
            o.push_str(" \"engine\": \"native\",");
        }
        o.push_str(&format!("\n     \"description\": {}}}{}\n", q(e.about), if i + 1 < all.len() { "," } else { "" }));
    }
    o.push_str("  ]\n}\n");
    o
}

/// The rules as a table (`--list-rules`).
pub fn rules_text() -> String {
    let mut o = format!(
        "\n=== Registered UVMCheck Rules ===\n\n{:<15}{:<32}{:<17}{:<10}Description\n{}\n",
        "Rule ID",
        "Name",
        "Category",
        "Severity",
        "-".repeat(118)
    );
    let all = entries();
    for e in &all {
        o.push_str(&format!("{:<15}{:<32}{:<17}{:<10}{}\n", e.id, e.name, e.category, e.severity.as_str(), e.about));
    }
    o.push_str(&format!("\n{} rules and sub-rules.\n", all.len()));
    o
}

/// A guideline with the rules that enforce it.
pub struct Guideline {
    /// `"MG-4.3"`.
    pub id: &'static str,
    /// `"MG-4"`.
    pub chapter: &'static str,
    /// The chapter's title.
    pub chapter_title: &'static str,
    /// Shall, Should or May.
    pub level: &'static str,
    /// The guideline itself.
    pub statement: &'static str,
    /// Ids of the rules enforcing it, sorted.
    pub rules: Vec<&'static str>,
}

/// Every methodology guideline, in guide order.
pub fn guidelines() -> Vec<Guideline> {
    let rules = crate::rules();
    GUIDELINES
        .iter()
        .map(|&(id, chapter_title, level, statement)| Guideline {
            id,
            chapter: id.split_once('.').map_or(id, |x| x.0),
            chapter_title,
            level,
            statement,
            rules: rules.iter().filter(|r| r.guidelines.contains(&id)).map(|r| r.id).collect(),
        })
        .collect()
}

/// The guide as JSON (`--list-guidelines --json`).
pub fn guidelines_json() -> String {
    let all = guidelines();
    let mut o = String::from("{\n  \"guidelines\": [\n");
    for (i, g) in all.iter().enumerate() {
        o.push_str(&format!(
            "    {{\"id\": {}, \"chapter\": {}, \"chapterTitle\": {}, \"level\": {}, \"statement\": {}, \"rules\": {}}}{}\n",
            q(g.id),
            q(g.chapter),
            q(g.chapter_title),
            q(g.level),
            q(g.statement),
            list(&g.rules),
            if i + 1 < all.len() { "," } else { "" }
        ));
    }
    o.push_str("  ]\n}\n");
    o
}

/// The guide as text (`--list-guidelines`).
pub fn guidelines_text() -> String {
    let mut o = String::new();
    let mut chapter = "";
    for g in guidelines() {
        if g.chapter != chapter {
            chapter = g.chapter;
            o.push_str(&format!("\n{} {}\n", g.chapter, g.chapter_title));
        }
        o.push_str(&format!("  {:<8}{:<8}{}\n", g.id, g.level, g.statement));
        let rules = if g.rules.is_empty() { "review only (no rule yet)".to_string() } else { g.rules.join(", ") };
        o.push_str(&format!("{}Rules: {rules}\n", " ".repeat(18)));
    }
    o
}
