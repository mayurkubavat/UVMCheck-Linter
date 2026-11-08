//! Report formats besides the text listing: SARIF 2.1.0, as uvmcheck's
//! SarifReporter writes it (the rule table, then one result per finding
//! with its level, message, remediation estimate, location and related
//! locations).

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
        o.push_str(&format!("\n        }}{}\n", if i + 1 < found.len() { "," } else { "" }));
    }
    o.push_str("      ]\n    }\n  ]\n}\n");
    o
}
