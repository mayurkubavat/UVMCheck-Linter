//! `--map FILE`: the testbench map, as uvmcheck's TestbenchMap writes it:
//! the component tree from each test (component-typed properties are
//! children), each component's TLM ports, connect() calls, config_db sets
//! and gets and the findings inside it; then the config_db flows by field
//! and every TLM connection. HTML, or JSON for a `.json` path.

use std::collections::{BTreeMap, BTreeSet, HashSet};

use crate::support::class_roles::{role, role_name};
use crate::support::uvm_classes::{is_component, user_classes};
use crate::tbgraph::{ConfigAccess, Connection, Graph, Port};
use crate::Finding;
use dv_model::Design;

struct Node<'g> {
    name: String,
    role: &'static str,
    file: String,
    first: u32,
    last: u32,
    children: Vec<(String, String)>,
    ports: Vec<&'g Port>,
    unconnected: BTreeSet<String>,
    connections: Vec<&'g Connection>,
    config: Vec<&'g ConfigAccess>,
    findings: Vec<(&'static str, &'static str, u32, String)>,
    contained: bool,
}

/// A component class: name, role, file, first and last line, children
/// (instance, class).
type ClassRow = (String, &'static str, String, u32, u32, Vec<(String, String)>);

/// The map's model, built before the findings are final.
pub struct Map {
    graph: Graph,
    classes: Vec<ClassRow>,
}

fn canonical(p: &str) -> String {
    std::fs::canonicalize(p).map_or_else(|_| p.to_string(), |c| c.display().to_string())
}

impl Map {
    /// The design's structure (see [`Graph::build`] for `trees`, `files`).
    pub fn new(design: &Design, trees: &[sv_syntax::Tree], files: &[String]) -> Map {
        let graph = Graph::build(design, trees, files);
        let mut classes = Vec::new();
        for c in user_classes(design) {
            if !is_component(design, c.id) || classes.iter().any(|x: &ClassRow| x.0 == c.name) {
                continue;
            }
            let at = crate::tbgraph::where_of(design.files(), c.loc);
            let (first, last) = match graph.spans.get(&c.name) {
                Some((a, b)) if a.file == at.file => (a.line, *b),
                _ => (at.line, at.line),
            };
            let children = design.child_components(c).map(|(p, t)| (p.name.clone(), t.name.clone())).collect();
            classes.push((c.name.clone(), role_name(role(design, c)), at.file, first, last, children));
        }
        Map { graph, classes }
    }

    /// Writes the map with the run's final findings.
    pub fn write(&self, path: &str, files: &[String], found: &[Finding]) -> std::io::Result<()> {
        let g = &self.graph;
        let mut nodes: BTreeMap<String, Node> = BTreeMap::new();
        for (name, role, file, first, last, children) in &self.classes {
            nodes.insert(
                name.clone(),
                Node {
                    name: name.clone(),
                    role,
                    file: file.clone(),
                    first: *first,
                    last: *last,
                    children: children.clone(),
                    ports: Vec::new(),
                    unconnected: BTreeSet::new(),
                    connections: Vec::new(),
                    config: Vec::new(),
                    findings: Vec::new(),
                    contained: false,
                },
            );
        }
        let contained: Vec<String> = nodes
            .values()
            .flat_map(|n| n.children.iter().filter(|(_, c)| *c != n.name).map(|(_, c)| c.clone()))
            .collect();
        for c in contained {
            if let Some(n) = nodes.get_mut(&c) {
                n.contained = true;
            }
        }
        let unconnected: HashSet<(&str, &str)> =
            g.unconnected().into_iter().map(|p| (p.component.as_str(), p.name.as_str())).collect();
        for p in &g.ports {
            if let Some(n) = nodes.get_mut(&p.component) {
                n.ports.push(p);
                if unconnected.contains(&(p.component.as_str(), p.name.as_str())) {
                    n.unconnected.insert(p.name.clone());
                }
            }
        }
        let mut connections = Vec::new();
        for k in &g.connections {
            connections.push(k);
            if let Some(n) = nodes.get_mut(&k.in_class) {
                n.connections.push(k);
            }
        }
        let mut sets: BTreeMap<String, Vec<&ConfigAccess>> = BTreeMap::new();
        let mut gets: BTreeMap<String, Vec<&ConfigAccess>> = BTreeMap::new();
        for k in &g.config {
            // Sets and gets of user code outside components too, not the library's.
            if !nodes.contains_key(&k.in_class) && (k.at.file.is_empty() || k.at.file.contains("uvm_")) {
                continue;
            }
            if k.set { &mut sets } else { &mut gets }.entry(k.field.clone()).or_default().push(k);
            if let Some(n) = nodes.get_mut(&k.in_class) {
                n.config.push(k);
            }
        }
        let mut by_file: BTreeMap<String, Vec<String>> = BTreeMap::new();
        for n in nodes.values() {
            by_file.entry(canonical(&n.file)).or_default().push(n.name.clone());
        }
        let mut canon_cache: BTreeMap<String, String> = BTreeMap::new();
        for f in found {
            let (file, line, _) = f.position_in(files);
            if file.is_empty() {
                continue;
            }
            let c = canon_cache.entry(file.to_string()).or_insert_with(|| canonical(file)).clone();
            let Some(names) = by_file.get(&c) else { continue };
            if let Some(name) = names.iter().find(|n| nodes.get(*n).is_some_and(|x| line >= x.first && line <= x.last))
            {
                if let Some(n) = nodes.get_mut(name) {
                    n.findings.push((f.rule, f.severity.as_str(), line, f.message.clone()));
                }
            }
        }
        let mut roots: Vec<String> = nodes.values().filter(|n| !n.contained).map(|n| n.name.clone()).collect();
        roots.sort_by_key(|r| nodes[r].role != "test");
        let m = Model { nodes, sets, gets, connections, roots };
        let text = if path.len() > 5 && path.ends_with(".json") { m.json() } else { m.html() };
        std::fs::write(path, text)
    }
}

struct Model<'g> {
    nodes: BTreeMap<String, Node<'g>>,
    sets: BTreeMap<String, Vec<&'g ConfigAccess>>,
    gets: BTreeMap<String, Vec<&'g ConfigAccess>>,
    connections: Vec<&'g Connection>,
    roots: Vec<String>,
}

fn html(s: &str) -> String {
    let mut o = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '<' => o.push_str("&lt;"),
            '>' => o.push_str("&gt;"),
            '&' => o.push_str("&amp;"),
            '"' => o.push_str("&quot;"),
            c => o.push(c),
        }
    }
    o
}

fn js(s: &str) -> String {
    let mut o = String::from("\"");
    for c in s.chars() {
        if c == '"' || c == '\\' {
            o.push('\\');
        }
        if c == '\n' {
            o.push_str("\\n");
            continue;
        }
        if (c as u32) < 0x20 {
            continue;
        }
        o.push(c);
    }
    o.push('"');
    o
}

const STYLE: &str = r#"<!doctype html><html lang="en"><head><meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>Testbench map</title>
<style>
:root{--bg:#fbfaf8;--fg:#1f2328;--muted:#6a6f76;--line:#e3e1dc;--card:#fff;--accent:#2f6fb2;--warn:#b25a00;--err:#b42318}
@media (prefers-color-scheme: dark){:root:not([data-theme="light"]){--bg:#16181b;--fg:#e6e6e3;--muted:#9aa0a6;--line:#2c3036;--card:#1d2024;--accent:#7fb0e8;--warn:#f0a35a;--err:#f2867c}}
body{background:var(--bg);color:var(--fg);font:14px/1.5 system-ui,-apple-system,Segoe UI,sans-serif;margin:0;padding:24px 16px}
main{max-width:1100px;margin:0 auto}h1{font-size:22px;margin:0 0 4px}h2{font-size:16px;margin:28px 0 8px}
.stats{color:var(--muted);margin-bottom:16px}details.fd{border:0;margin:0;padding:0}details{border-left:2px solid var(--line);margin:4px 0 4px 6px;padding-left:10px}
summary{cursor:pointer;padding:3px 0}.inst{font-weight:600}.cls{color:var(--muted)}.body{padding:2px 0 6px 4px}
.role{font-size:11px;border:1px solid var(--line);border-radius:10px;padding:0 7px;color:var(--accent)}
.badge{font-size:11px;border-radius:10px;padding:0 7px;background:var(--line)}.badge.warn{color:var(--warn)}
.sec{font-size:11px;text-transform:uppercase;letter-spacing:.04em;color:var(--muted);margin-top:6px}
ul{margin:2px 0;padding-left:18px}.muted,.where{color:var(--muted);font-size:12px}code{font-size:12px}
table{border-collapse:collapse;width:100%;background:var(--card)}td,th{border-bottom:1px solid var(--line);padding:5px 8px;text-align:left;font-size:13px;vertical-align:top}
.sev{font-weight:600;font-size:12px}.sev-ERROR{color:var(--err)}.sev-WARNING{color:var(--warn)}.wrap{overflow-x:auto}
</style></head><body><main>
<h1>Testbench map</h1>
"#;

fn count(n: usize, one: &str, many: &str) -> String {
    format!("{n} {}", if n == 1 { one } else { many })
}

impl Model<'_> {
    fn render(&self, o: &mut String, inst: &str, cls: &str, path: &mut BTreeSet<String>, depth: usize) {
        let Some(n) = self.nodes.get(cls) else { return };
        o.push_str(&format!(
            "<details{}><summary><span class=\"inst\">{}</span> <span class=\"cls\">{}</span> <span class=\"role role-{}\">{}</span>",
            if depth < 3 { " open" } else { "" },
            html(inst),
            html(&n.name),
            html(n.role),
            html(n.role)
        ));
        if !n.findings.is_empty() {
            o.push_str(&format!(" <span class=\"badge\">{}</span>", count(n.findings.len(), "finding", "findings")));
        }
        if !n.unconnected.is_empty() {
            o.push_str(&format!(" <span class=\"badge warn\">{} unconnected</span>", n.unconnected.len()));
        }
        o.push_str(&format!("</summary><div class=\"body\"><div class=\"where\">{}:{}</div>", html(&n.file), n.first));
        if !n.ports.is_empty() {
            o.push_str("<div class=\"sec\">Ports</div><ul>");
            for p in &n.ports {
                o.push_str(&format!(
                    "<li><code>{}</code> <span class=\"muted\">{}</span>{}</li>",
                    html(&p.name),
                    html(&p.ty),
                    if n.unconnected.contains(&p.name) {
                        " <span class=\"badge warn\">never connected</span>"
                    } else {
                        ""
                    }
                ));
            }
            o.push_str("</ul>");
        }
        if !n.connections.is_empty() {
            o.push_str("<div class=\"sec\">Connects</div><ul>");
            for k in &n.connections {
                o.push_str(&format!(
                    "<li><code>{}</code> &rarr; <code>{}</code></li>",
                    html(&k.source),
                    html(&k.target)
                ));
            }
            o.push_str("</ul>");
        }
        if !n.config.is_empty() {
            o.push_str("<div class=\"sec\">config_db</div><ul>");
            for k in &n.config {
                let matched = if k.set { self.gets.contains_key(&k.field) } else { self.sets.contains_key(&k.field) };
                let badge = match (matched, k.set) {
                    (true, _) => "",
                    (false, true) => " <span class=\"badge warn\">never read</span>",
                    (false, false) => " <span class=\"badge warn\">no set()</span>",
                };
                let inst = if k.set && !k.inst.is_empty() { format!(" for {}", html(&k.inst)) } else { String::new() };
                o.push_str(&format!(
                    "<li>{}<code>{}</code> <span class=\"muted\">{}{inst}</span>{badge}</li>",
                    if k.set { "set " } else { "get " },
                    html(&k.field),
                    html(&k.ty)
                ));
            }
            o.push_str("</ul>");
        }
        if !n.findings.is_empty() {
            o.push_str(&format!(
                "<details class=\"fd\"><summary class=\"sec\">Findings ({})</summary><ul class=\"findings\">",
                n.findings.len()
            ));
            for (rule, sev, line, msg) in &n.findings {
                o.push_str(&format!("<li><span class=\"sev sev-{sev}\">{rule}</span> line {line}: {}</li>", html(msg)));
            }
            o.push_str("</ul></details>");
        }
        if !n.children.is_empty() {
            o.push_str("<div class=\"sec\">Children</div>");
            for (ci, cc) in &n.children {
                // A recursive structure is shown once.
                if path.contains(cc) {
                    continue;
                }
                path.insert(cc.clone());
                self.render(o, ci, cc, path, depth + 1);
                path.remove(cc);
            }
        }
        o.push_str("</div></details>\n");
    }

    fn html(&self) -> String {
        let findings: usize = self.nodes.values().map(|n| n.findings.len()).sum();
        let unconnected: usize = self.nodes.values().map(|n| n.unconnected.len()).sum();
        let orphan_gets: usize =
            self.gets.iter().filter(|(f, _)| !self.sets.contains_key(*f)).map(|(_, v)| v.len()).sum();
        let unread_sets: usize =
            self.sets.iter().filter(|(f, _)| !self.gets.contains_key(*f)).map(|(_, v)| v.len()).sum();
        let mut o = String::from(STYLE);
        o.push_str(&format!(
            "<div class=\"stats\">{} &middot; {} &middot; {} &middot; {} &middot; {} &middot; {}</div>\n<h2>Component tree</h2>\n",
            count(self.nodes.len(), "component class", "component classes"),
            count(self.connections.len(), "TLM connection", "TLM connections"),
            count(unconnected, "unconnected port", "unconnected ports"),
            count(orphan_gets, "config_db get without a set", "config_db gets without a set"),
            count(unread_sets, "set never read", "sets never read"),
            count(findings, "finding inside components", "findings inside components")
        ));
        for r in &self.roots {
            let mut path = BTreeSet::from([r.clone()]);
            let inst = if self.nodes[r].role == "test" { "uvm_test_top" } else { r.as_str() };
            self.render(&mut o, inst, r, &mut path, 0);
        }
        o.push_str("<h2>config_db flows</h2><div class=\"wrap\"><table><tr><th>Field</th><th>set() in</th><th>get() in</th></tr>\n");
        let fields: BTreeSet<&String> = self.sets.keys().chain(self.gets.keys()).collect();
        let list = |by: &BTreeMap<String, Vec<&ConfigAccess>>, f: &str| -> String {
            let Some(v) = by.get(f) else { return "<span class=\"badge warn\">none</span>".into() };
            let mut seen = Vec::new();
            for k in v {
                let l = if k.in_class.is_empty() { "(top)".to_string() } else { k.in_class.clone() };
                if !seen.contains(&l) {
                    seen.push(l);
                }
            }
            seen.iter().map(|s| html(s)).collect::<Vec<_>>().join(", ")
        };
        for f in fields {
            o.push_str(&format!(
                "<tr><td><code>{}</code></td><td>{}</td><td>{}</td></tr>\n",
                html(f),
                list(&self.sets, f),
                list(&self.gets, f)
            ));
        }
        o.push_str("</table></div>\n<h2>TLM connections</h2><div class=\"wrap\"><table><tr><th>In</th><th>From</th><th>To</th></tr>\n");
        for k in &self.connections {
            o.push_str(&format!(
                "<tr><td>{}</td><td><code>{}</code></td><td><code>{}</code></td></tr>\n",
                html(&k.in_class),
                html(&k.source),
                html(&k.target)
            ));
        }
        o.push_str("</table></div>\n</main></body></html>\n");
        o
    }

    fn json(&self) -> String {
        let mut o = String::from("{\n  \"version\": 1,\n  \"roots\": [");
        o.push_str(&self.roots.iter().map(|r| js(r)).collect::<Vec<_>>().join(", "));
        o.push_str("],\n  \"components\": [");
        for (i, (name, n)) in self.nodes.iter().enumerate() {
            let children: Vec<String> =
                n.children.iter().map(|(a, b)| format!("{{\"instance\": {}, \"class\": {}}}", js(a), js(b))).collect();
            let ports: Vec<String> = n
                .ports
                .iter()
                .map(|p| {
                    format!(
                        "{{\"name\": {}, \"type\": {}, \"connected\": {}}}",
                        js(&p.name),
                        js(&p.ty),
                        !n.unconnected.contains(&p.name)
                    )
                })
                .collect();
            let findings: Vec<String> =
                n.findings.iter().map(|(r, _, l, _)| format!("{{\"rule\": {}, \"line\": {l}}}", js(r))).collect();
            o.push_str(&format!(
                "{}    {{\"name\": {}, \"role\": {}, \"file\": {}, \"line\": {}, \"children\": [{}], \"ports\": [{}], \"findings\": [{}]}}",
                if i == 0 { "\n" } else { ",\n" },
                js(name),
                js(n.role),
                js(&n.file),
                n.first,
                children.join(", "),
                ports.join(", "),
                findings.join(", ")
            ));
        }
        o.push_str("\n  ],\n  \"connections\": [");
        for (i, k) in self.connections.iter().enumerate() {
            o.push_str(&format!(
                "{}{{\"in\": {}, \"from\": {}, \"to\": {}}}",
                if i == 0 { "\n    " } else { ",\n    " },
                js(&k.in_class),
                js(&k.source),
                js(&k.target)
            ));
        }
        o.push_str("\n  ],\n  \"config_db\": [");
        let mut first = true;
        for (set, by, other) in [(true, &self.sets, &self.gets), (false, &self.gets, &self.sets)] {
            for (f, v) in by {
                for k in v {
                    o.push_str(&format!(
                        "{}{{\"op\": \"{}\", \"field\": {}, \"type\": {}, \"in\": {}, \"matched\": {}}}",
                        if first { "\n    " } else { ",\n    " },
                        if set { "set" } else { "get" },
                        js(f),
                        js(&k.ty),
                        js(&k.in_class),
                        other.contains_key(f)
                    ));
                    first = false;
                }
            }
        }
        o.push_str("\n  ]\n}\n");
        o
    }
}
