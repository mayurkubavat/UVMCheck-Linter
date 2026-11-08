//! Report formats besides the text listing: SARIF 2.1.0, as uvmcheck's
//! SarifReporter writes it (the rule table, then one result per finding
//! with its level, message, remediation estimate, location, related
//! locations and fixes), and the HTML dashboard.

use crate::json::Json;
use crate::{Finding, Severity};

fn s(text: &str) -> String {
    Json::Str(text.to_string()).to_string()
}

fn level(sev: Severity) -> &'static str {
    match sev {
        Severity::Error => "error",
        Severity::Warning | Severity::Style => "warning",
        Severity::Info => "note",
    }
}

/// uvmcheck's remediation estimate, in minutes.
fn remediation(f: &Finding) -> u32 {
    match f.rule {
        "SV-SML-004" => 60,
        "SV-SML-002" => 30,
        "SV-SML-001" => 20,
        "SV-PRF-007" | "SV-PRF-008" => 5,
        "SV-PRF-005" => 15,
        "SV-PRF-009" => 10,
        _ => match f.severity {
            Severity::Error => 45,
            Severity::Warning | Severity::Style => 15,
            Severity::Info => 5,
        },
    }
}

/// The SARIF 2.1.0 report of `found`.
pub fn sarif(files: &[String], found: &[Finding]) -> String {
    let mut o = String::from(
        "{\n  \"$schema\": \"https://raw.githubusercontent.com/oasis-tcs/sarif-spec/master/Schemata/sarif-schema-2.1.0.json\",\n  \"version\": \"2.1.0\",\n  \"runs\": [\n    {\n      \"tool\": {\n        \"driver\": {\n",
    );
    o.push_str(&format!(
        "          \"name\": \"dv-lint\",\n          \"version\": {},\n          \"informationUri\": \"https://github.com/uvmcheck/uvmcheck\",\n          \"rules\": [\n",
        s(env!("CARGO_PKG_VERSION"))
    ));
    let docs = crate::rule_profiles::DOCS;
    let cats = crate::rule_profiles::RULES;
    for (i, (id, name, desc)) in docs.iter().enumerate() {
        let cat = cats.iter().find(|r| r.0 == *id).map_or("", |r| r.1);
        o.push_str(&format!(
            "            {{\n              \"id\": {},\n              \"name\": {},\n              \"shortDescription\": {{ \"text\": {} }},\n              \"properties\": {{ \"category\": {} }}\n            }}{}\n",
            s(id),
            s(name),
            s(desc),
            s(cat),
            if i + 1 < docs.len() { "," } else { "" }
        ));
    }
    o.push_str("          ]\n        }\n      },\n      \"results\": [\n");
    let mut pos = crate::fix::Positions::default();
    for (i, f) in found.iter().enumerate() {
        let (file, line, column) = f.position_in(files);
        o.push_str(&format!(
            "        {{\n          \"ruleId\": {},\n          \"level\": \"{}\",\n          \"message\": {{ \"text\": {} }},\n          \"properties\": {{ \"remediationMinutes\": {} }},\n          \"locations\": [\n            {{\n              \"physicalLocation\": {{\n                \"artifactLocation\": {{ \"uri\": {} }},\n                \"region\": {{\n                  \"startLine\": {line},\n                  \"startColumn\": {column}\n                }}\n              }}\n            }}\n          ]",
            s(f.rule),
            level(f.severity),
            s(&f.message),
            remediation(f),
            s(file)
        ));
        if !f.related.is_empty() {
            o.push_str(",\n          \"relatedLocations\": [");
            for (r, (loc, note)) in f.related.iter().enumerate() {
                let nfile = files.get(loc.file as usize).map(String::as_str).unwrap_or("");
                o.push_str(&format!(
                    "{}\n            {{ \"id\": {r}, \"message\": {{ \"text\": {} }}, \"physicalLocation\": {{ \"artifactLocation\": {{ \"uri\": {} }}, \"region\": {{ \"startLine\": {}, \"startColumn\": {} }} }} }}",
                    if r > 0 { "," } else { "" },
                    s(note),
                    s(nfile),
                    loc.line,
                    loc.column
                ));
            }
            o.push_str("\n          ]");
        }
        if !f.fixes.is_empty() {
            o.push_str(",\n          \"fixes\": [");
            for (k, x) in f.fixes.iter().enumerate() {
                let (l1, c1) = pos.at(&x.path, x.start);
                let (l2, c2) = pos.at(&x.path, x.end);
                o.push_str(&format!(
                    "{}\n            {{\n              \"description\": {{ \"text\": {} }},\n              \"properties\": {{ \"safety\": \"{}\" }},\n              \"artifactChanges\": [{{\n                \"artifactLocation\": {{ \"uri\": {} }},\n                \"replacements\": [{{\n                  \"deletedRegion\": {{ \"startLine\": {l1}, \"startColumn\": {c1}, \"endLine\": {l2}, \"endColumn\": {c2} }},\n                  \"insertedContent\": {{ \"text\": {} }}\n                }}]\n              }}]\n            }}",
                    if k > 0 { "," } else { "" },
                    s(&x.description),
                    x.safety.as_str(),
                    s(file),
                    s(&x.text)
                ));
            }
            o.push_str("\n          ]");
        }
        o.push_str(&format!("\n        }}{}\n", if i + 1 < found.len() { "," } else { "" }));
    }
    o.push_str("      ]\n    }\n  ]\n}\n");
    o
}

fn html_escape(s: &str) -> String {
    let mut o = String::with_capacity(s.len() + s.len() / 5);
    for ch in s.chars() {
        match ch {
            '&' => o.push_str("&amp;"),
            '<' => o.push_str("&lt;"),
            '>' => o.push_str("&gt;"),
            '"' => o.push_str("&quot;"),
            '\'' => o.push_str("&#39;"),
            c => o.push(c),
        }
    }
    o
}

/// The time now, UTC, as `YYYY-MM-DD HH:MM:SS UTC`.
fn now_utc() -> String {
    let secs = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs()) as i64;
    let (days, rem) = (secs.div_euclid(86_400), secs.rem_euclid(86_400));
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(m <= 2);
    format!("{y:04}-{m:02}-{d:02} {:02}:{:02}:{:02} UTC", rem / 3600, rem / 60 % 60, rem % 60)
}

/// The HTML quality dashboard of `found`, as uvmcheck's HtmlReporter writes
/// it: KPI cards (findings, maintainability grade from the remediation
/// estimate, technical debt, errors and warnings, files), findings by
/// category and the top rules, then a filterable table with each finding's
/// source line and fix hints. The most-complex-routines table is not
/// computed here.
pub fn html(files: &[String], found: &[Finding]) -> String {
    use std::collections::{HashMap, HashSet};
    let category = |id: &str| -> &'static str {
        let parent = crate::rules()
            .iter()
            .find(|r| crate::sub_rules(r.id).iter().any(|s| s.0 == id))
            .map(|r| r.id)
            .unwrap_or(id);
        crate::rule_profiles::RULES.iter().find(|r| r.0 == parent).map_or("Custom", |r| r.1)
    };
    let (mut errors, mut warnings, mut infos, mut debt) = (0usize, 0usize, 0usize, 0u64);
    let mut by_cat: HashMap<&str, usize> = HashMap::new();
    let mut by_rule: HashMap<&str, usize> = HashMap::new();
    let mut uniq: HashSet<&str> = HashSet::new();
    let mut cats = Vec::with_capacity(found.len());
    for f in found {
        match f.severity {
            Severity::Error => errors += 1,
            Severity::Warning | Severity::Style => warnings += 1,
            Severity::Info => infos += 1,
        }
        debt += u64::from(remediation(f));
        let c = category(f.rule);
        cats.push(c);
        *by_cat.entry(c).or_default() += 1;
        *by_rule.entry(f.rule).or_default() += 1;
        uniq.insert(f.position_in(files).0);
    }
    let hours = debt as f64 / 60.0;
    let (grade, color) = match hours {
        h if h > 20.0 => ("E", "#ef4444"),
        h if h > 10.0 => ("D", "#f97316"),
        h if h > 5.0 => ("C", "#eab308"),
        h if h > 2.0 => ("B", "#3b82f6"),
        _ => ("A", "#10b981"),
    };
    let mut o = String::new();
    o.push_str("<!DOCTYPE html>\n<html lang=\"en\">\n<head>\n  <meta charset=\"UTF-8\">\n  <meta name=\"viewport\" content=\"width=device-width, initial-scale=1.0\">\n  <title>UVMCheck Quality Dashboard</title>\n  <style>\n");
    o.push_str(include_str!("report_html.css"));
    o.push_str(
        "  </style>\n</head>\n<body>\n  <div class=\"container\">\n    <header>\n      <div class=\"logo-group\">\n",
    );
    o.push_str(&format!(
        "        <h1>UVMCheck Quality Dashboard <span class=\"badge-pill\">dv-lint {}</span></h1>\n        <div class=\"meta-time\">SystemVerilog &amp; UVM Static Analysis &bull; Generated {}</div>\n      </div>\n    </header>\n\n",
        env!("CARGO_PKG_VERSION"),
        now_utc()
    ));
    let card = |title: &str, val: &str| {
        format!("      <div class=\"card\">\n        <div class=\"card-title\">{title}</div>\n        <div class=\"card-val\"{val}</div>\n      </div>\n")
    };
    o.push_str("    <div class=\"grid-kpi\">\n");
    o.push_str(&card("Total Violations", &format!(">{}", found.len())));
    o.push_str(&card("Maintainability Rating", &format!(" style=\"color: {color};\">Grade {grade}")));
    o.push_str(&card(
        "Technical Debt",
        &format!(
            ">{hours:.1} <span style=\"font-size: 0.9rem; color: var(--text-muted); font-weight: normal;\">hrs</span>"
        ),
    ));
    o.push_str(&card(
        "Errors / Warnings",
        &format!("><span class=\"val-err\">{errors}</span> / <span class=\"val-warn\">{warnings}</span>"),
    ));
    o.push_str(&format!(
        "      <div class=\"card\">\n        <div class=\"card-title\">Files Checked</div>\n        <div class=\"card-val val-info\">{}</div>\n      </div>\n    </div>\n\n",
        uniq.len()
    ));
    o.push_str("    <div class=\"analytics-section\">\n      <div class=\"card\">\n        <div class=\"card-title\">Violations by Category</div>\n");
    let max = by_cat.values().copied().max().unwrap_or(1).max(1);
    let mut cat_rows: Vec<(&str, usize)> = by_cat.into_iter().collect();
    cat_rows.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(b.0)));
    for (c, n) in &cat_rows {
        o.push_str(&format!(
            "        <div class=\"bar-row\">\n          <span class=\"bar-label\">{c}</span>\n          <div class=\"bar-track\"><div class=\"bar-fill\" style=\"width: {}%\"></div></div>\n          <span class=\"bar-count\">{n}</span>\n        </div>\n",
            *n as f64 / max as f64 * 100.0
        ));
    }
    o.push_str("      </div>\n      <div class=\"card\">\n        <div class=\"card-title\">Top Rules Flagged</div>\n");
    let mut rules: Vec<(&str, usize)> = by_rule.into_iter().collect();
    rules.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(b.0)));
    for (r, n) in rules.iter().take(5) {
        o.push_str(&format!(
            "        <div style=\"display:flex; justify-content:space-between; margin-bottom: 0.5rem; font-size: 0.875rem;\">\n          <span class=\"rule-id\">{r}</span>\n          <span style=\"font-weight:600;\">{n}</span>\n        </div>\n"
        ));
    }
    if rules.is_empty() {
        o.push_str(
            "        <div style=\"color: var(--text-muted); font-size: 0.875rem;\">No violations detected!</div>\n",
        );
    }
    o.push_str("      </div>\n    </div>\n\n");
    o.push_str("    <h2 style=\"font-size:1.05rem; margin: 0 0 0.75rem;\">Most complex routines</h2>\n    <table id=\"complexityTable\" style=\"margin-bottom: 1.5rem;\">\n      <thead><tr><th>Cognitive</th><th>Lines</th><th>Routine</th><th>Location</th></tr></thead>\n      <tbody>\n        <tr><td colspan=\"4\" style=\"color: var(--text-muted);\">Not computed by the native engine yet.</td></tr>\n      </tbody>\n    </table>\n\n");
    o.push_str(&format!(
        "    <div class=\"filter-bar\">\n      <div class=\"search-box\">\n        <input type=\"text\" id=\"searchInput\" placeholder=\"Search rules, files, descriptions...\">\n      </div>\n      <div class=\"btn-group\">\n        <button class=\"filter-btn active\" data-sev=\"All\">All ({})</button>\n        <button class=\"filter-btn\" data-sev=\"Error\">Errors ({errors})</button>\n        <button class=\"filter-btn\" data-sev=\"Warning\">Warnings ({warnings})</button>\n        <button class=\"filter-btn\" data-sev=\"Info\">Info ({infos})</button>\n      </div>\n    </div>\n\n",
        found.len()
    ));
    o.push_str("    <table id=\"violationsTable\">\n      <thead>\n        <tr>\n          <th style=\"width: 100px;\">Severity</th>\n          <th style=\"width: 130px;\">Rule ID</th>\n          <th style=\"width: 180px;\">Category / Name</th>\n          <th>Location &amp; Message</th>\n        </tr>\n      </thead>\n      <tbody>\n");
    let mut texts: HashMap<&str, Vec<String>> = HashMap::new();
    for (i, f) in found.iter().enumerate() {
        let sev = match f.severity {
            Severity::Error => "Error",
            Severity::Warning | Severity::Style => "Warning",
            Severity::Info => "Info",
        };
        let (file, line, column) = f.position_in(files);
        o.push_str(&format!(
            "        <tr class=\"violation-row\" data-sev=\"{sev}\">\n          <td><span class=\"sev-pill sev-{sev}\">{sev}</span></td>\n          <td><span class=\"rule-id\">{}</span></td>\n          <td>\n            <div style=\"font-weight: 600;\">{}</div>\n            <div style=\"color: var(--text-muted); font-size: 0.75rem;\">{}</div>\n          </td>\n          <td>\n            <div class=\"loc-text\">{}:{line}:{column}</div>\n            <div style=\"margin-top: 0.25rem;\">{}</div>\n",
            html_escape(f.rule),
            html_escape(f.name),
            html_escape(cats[i]),
            html_escape(file),
            html_escape(&f.message)
        ));
        for x in &f.fixes {
            let what = if x.description.is_empty() { &x.text } else { &x.description };
            o.push_str(&format!(
                "            <div class=\"fix-hint\" data-safety=\"{0}\">Fix ({0}): {1}</div>\n",
                x.safety.as_str(),
                html_escape(what)
            ));
        }
        let lines = texts.entry(file).or_insert_with(|| {
            std::fs::read_to_string(file).map(|t| t.lines().map(str::to_string).collect()).unwrap_or_default()
        });
        if let Some(snippet) = (line as usize).checked_sub(1).and_then(|l| lines.get(l)).filter(|s| !s.is_empty()) {
            o.push_str(&format!(
                "            <button class=\"toggle-code-btn\" onclick=\"toggleCode('code_{i}')\">Show Snippet</button>\n            <div class=\"code-snippet\" id=\"code_{i}\" style=\"display: none;\">{}</div>\n",
                html_escape(snippet)
            ));
        }
        o.push_str("          </td>\n        </tr>\n");
    }
    o.push_str("      </tbody>\n    </table>\n  </div>\n\n  <script>\n");
    o.push_str(include_str!("report_html.js"));
    o.push_str("  </script>\n</body>\n</html>\n");
    o
}
