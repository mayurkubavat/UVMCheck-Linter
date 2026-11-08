//! `--python-rules PATH`: custom rules written against the uvmcheck Python
//! SDK (python/uvmcheck), run as uvmcheck runs them. The design is written
//! as the SDK's model JSON (version 2: user classes with their UVM role,
//! bases, properties, methods, arguments and calls; config_db accesses; TLM
//! ports and connections; component paths; per-rule options from the
//! project config) to a private temporary file, `python3 -m
//! uvmcheck.plugin_runner --rules PATH --model FILE` runs the rules, and the
//! findings it prints become findings of this run.
//!
//! The native model does not keep the text of a call's arguments, so each
//! call's `arguments` list is empty, and it does not tell `rand` from
//! `randc` (both are `"rand"`).

use crate::json::{obj, parse, Json};
use crate::support::class_roles::{class_methods, role, role_name, uvm_base_name};
use crate::support::uvm_classes::{derived, is_component, is_sequence, user_classes};
use crate::tbgraph::{where_of, Graph, Where};
use crate::{Finding, Severity};
use dv_model::{Design, Direction, Loc, Visibility};

fn at(w: &Where) -> Vec<(&'static str, Json)> {
    vec![("filename", w.file.as_str().into()), ("line", w.line.max(1).into()), ("column", w.column.max(1).into())]
}

fn with(mut v: Vec<(&'static str, Json)>, w: &Where) -> Json {
    v.extend(at(w));
    obj(v)
}

fn visibility(v: Visibility) -> &'static str {
    match v {
        Visibility::Local => "local",
        Visibility::Protected => "protected",
        Visibility::Public => "public",
    }
}

fn direction(d: Direction) -> &'static str {
    match d {
        Direction::Out => "output",
        Direction::InOut => "inout",
        Direction::Ref => "ref",
        Direction::In => "input",
    }
}

/// The SDK's model JSON (version 2) of `design`. `options` are the
/// config's rule options (rule key, JSON object text).
pub fn model_json(design: &Design, graph: &Graph, options: &[(String, String)]) -> String {
    let w = |l: Loc| where_of(design.files(), l);
    let mut classes = Vec::new();
    for c in user_classes(design) {
        let bases: Vec<Json> = design.ancestors(c.id).take(64).map(|a| a.name.as_str().into()).collect();
        let registered = c.typedefs.iter().any(|t| t.name == "type_id")
            || c.macros.iter().any(|m| m.name.ends_with("_utils") || m.name.ends_with("_utils_begin"));
        let props: Vec<Json> = c
            .properties
            .iter()
            .map(|p| {
                with(
                    vec![
                        ("name", p.name.as_str().into()),
                        ("type", p.ty.as_str().into()),
                        ("rand", if p.is_rand { "rand".into() } else { Json::Null }),
                        ("is_static", p.is_static.into()),
                        ("visibility", visibility(p.visibility).into()),
                    ],
                    &w(p.loc),
                )
            })
            .collect();
        let methods: Vec<Json> = class_methods(c)
            .map(|m| {
                let args: Vec<Json> = m
                    .arguments
                    .iter()
                    .map(|a| {
                        obj(vec![
                            ("name", a.name.as_str().into()),
                            ("type", a.ty.as_str().into()),
                            ("direction", direction(a.direction).into()),
                        ])
                    })
                    .collect();
                let calls: Vec<Json> = m
                    .calls
                    .iter()
                    .filter(|k| !k.is_system)
                    .map(|k| {
                        with(
                            vec![
                                ("name", k.name.as_str().into()),
                                ("receiver", k.receiver.as_str().into()),
                                ("declared_in", k.callee_scope.as_str().into()),
                            ],
                            &w(k.loc),
                        )
                        .push_field("arguments", Json::Arr(Vec::new()))
                    })
                    .collect();
                with(
                    vec![
                        ("name", m.name.as_str().into()),
                        ("kind", if m.is_task { "task" } else { "function" }.into()),
                        ("is_virtual", m.is_virtual.into()),
                        ("is_static", m.is_static.into()),
                        ("visibility", visibility(m.visibility).into()),
                        ("return_type", m.return_type.as_str().into()),
                    ],
                    &w(m.loc),
                )
                .push_field("arguments", Json::Arr(args))
                .push_field("calls", Json::Arr(calls))
            })
            .collect();
        let first = bases.first().cloned().unwrap_or_else(|| "".into());
        let r = role(design, c);
        let item = derived(design, c.id, "uvm_sequence_item");
        classes.push(
            with(
                vec![
                    ("name", c.name.as_str().into()),
                    ("base_class", first),
                    ("bases", Json::Arr(bases)),
                    ("role", role_name(r).into()),
                    ("uvm_base", uvm_base_name(design, c).into()),
                    ("factory_registered", registered.into()),
                    ("is_component", is_component(design, c.id).into()),
                    ("is_object", derived(design, c.id, "uvm_object").into()),
                    ("is_sequence", is_sequence(design, c.id).into()),
                    ("is_sequence_item", item.into()),
                    ("is_virtual", c.is_virtual.into()),
                ],
                &w(c.loc),
            )
            .push_field("properties", Json::Arr(props))
            .push_field("methods", Json::Arr(methods)),
        );
    }
    let user = |f: &str| !f.is_empty() && !f.contains("uvm_mock.svh") && !f.contains("uvm_pkg");
    let config: Vec<Json> = graph
        .config
        .iter()
        .filter(|k| user(&k.at.file))
        .map(|k| {
            with(
                vec![
                    ("op", if k.set { "set" } else { "get" }.into()),
                    ("type", k.ty.as_str().into()),
                    ("in_class", k.in_class.as_str().into()),
                    ("inst_name", k.inst.as_str().into()),
                    ("field_name", k.field.as_str().into()),
                ],
                &k.at,
            )
        })
        .collect();
    let ports: Vec<Json> = graph
        .ports
        .iter()
        .map(|p| {
            with(
                vec![
                    ("component", p.component.as_str().into()),
                    ("name", p.name.as_str().into()),
                    ("type", p.ty.as_str().into()),
                    ("is_pull", p.is_pull.into()),
                    ("is_analysis", p.is_analysis.into()),
                ],
                &p.at,
            )
        })
        .collect();
    let conns: Vec<Json> = graph
        .connections
        .iter()
        .map(|k| {
            with(
                vec![
                    ("in_class", k.in_class.as_str().into()),
                    ("source", k.source.as_str().into()),
                    ("target", k.target.as_str().into()),
                ],
                &k.at,
            )
        })
        .collect();
    let paths: Vec<Json> = component_paths(design).into_iter().map(Json::from).collect();
    let opts = Json::Obj(
        options.iter().filter_map(|(k, v)| parse(v).ok().map(|j| (k.clone(), j))).collect::<Vec<(String, Json)>>(),
    );
    obj(vec![
        ("version", 2u32.into()),
        ("classes", Json::Arr(classes)),
        ("config_db", Json::Arr(config)),
        ("tlm_ports", Json::Arr(ports)),
        ("tlm_connections", Json::Arr(conns)),
        ("component_paths", Json::Arr(paths)),
        ("rule_options", opts),
    ])
    .to_string()
}

/// uvmcheck's component paths (UvmModel::buildComponentDag): each
/// component-typed property of every component class, as `name`,
/// `uvm_test_top.name` and `uvm_test_top.m_env.name`.
fn component_paths(design: &Design) -> Vec<String> {
    let mut out = Vec::new();
    for c in design.classes().iter().filter(|c| is_component(design, c.id)) {
        for (p, _) in design.child_components(c) {
            out.extend([p.name.clone(), format!("uvm_test_top.{}", p.name), format!("uvm_test_top.m_env.{}", p.name)]);
        }
    }
    out
}

trait PushField {
    fn push_field(self, k: &str, v: Json) -> Json;
}

impl PushField for Json {
    fn push_field(mut self, k: &str, v: Json) -> Json {
        if let Json::Obj(kv) = &mut self {
            kv.push((k.to_string(), v));
        }
        self
    }
}

/// Runs the Python rules at `path` on `design` and returns their findings,
/// numbering new files in `files`. Failures are reported on stderr, as
/// uvmcheck's (`[uvmcheck-plugins] ...`), and give what the rules that ran
/// found.
pub fn run(
    path: &str,
    design: &Design,
    trees: &[sv_syntax::Tree],
    files: &mut Vec<String>,
    options: &[(String, String)],
) -> Vec<Finding> {
    let graph = Graph::build(design, trees, files);
    let model = std::env::temp_dir().join(format!("dv_lint_model_{}.json", std::process::id()));
    if let Err(e) = std::fs::write(&model, model_json(design, &graph, options)) {
        eprintln!("[uvmcheck-plugins] Error: cannot write a temporary model file: {e}");
        return Vec::new();
    }
    let python = std::env::var("UVMCHECK_PYTHON").unwrap_or_else(|_| "python3".into());
    let mut cmd = std::process::Command::new(&python);
    cmd.args(["-m", "uvmcheck.plugin_runner", "--rules", path, "--model"]).arg(&model);
    if let Some(sdk) = crate::project::python_sdk() {
        let mut pp = sdk.into_os_string();
        if let Some(old) = std::env::var_os("PYTHONPATH").filter(|o| !o.is_empty()) {
            pp.push(":");
            pp.push(old);
        }
        cmd.env("PYTHONPATH", pp);
    }
    cmd.stderr(std::process::Stdio::inherit());
    let out = cmd.output();
    let _ = std::fs::remove_file(&model);
    let out = match out {
        Ok(o) => o,
        Err(e) => {
            eprintln!("[uvmcheck-plugins] Python rule runner failed: cannot start {python}: {e}");
            return Vec::new();
        }
    };
    match out.status.code() {
        Some(0) => {}
        // Some rule raised (its traceback is on stderr); the others reported.
        Some(2) => eprintln!("[uvmcheck-plugins] Some Python rules failed; their findings are missing"),
        c => {
            eprintln!("[uvmcheck-plugins] Python rule runner failed (exit code {})", c.unwrap_or(-1));
            return Vec::new();
        }
    }
    match parse(&String::from_utf8_lossy(&out.stdout)) {
        Ok(Json::Arr(items)) => items.iter().filter_map(|r| finding(r, files)).collect(),
        Ok(_) => {
            eprintln!("[uvmcheck-plugins] Python rule runner printed invalid JSON (expected a list)");
            Vec::new()
        }
        Err(e) => {
            eprintln!("[uvmcheck-plugins] Python rule runner printed invalid JSON ({e})");
            Vec::new()
        }
    }
}

fn leak(s: &str) -> &'static str {
    Box::leak(s.to_string().into_boxed_str())
}

fn file_index(files: &mut Vec<String>, name: &str) -> u32 {
    if name.is_empty() {
        return 0;
    }
    if let Some(i) = files.iter().position(|f| f == name) {
        return i as u32;
    }
    files.push(name.to_string());
    files.len() as u32 - 1
}

/// Byte offset of 1-based `line`, `column` in `text`.
fn offset(text: &str, line: u32, column: u32) -> u32 {
    let start: usize = text.split_inclusive('\n').take(line.saturating_sub(1) as usize).map(str::len).sum();
    (start + column.saturating_sub(1) as usize).min(text.len()) as u32
}

fn finding(r: &Json, files: &mut Vec<String>) -> Option<Finding> {
    let id = r.get("rule_id").as_str().filter(|s| !s.is_empty())?;
    let name = r.get("rule_name").as_str().filter(|s| !s.is_empty()).unwrap_or(id);
    let num = |k: &str| r.get(k).as_u32().filter(|&n| n >= 1).unwrap_or(1);
    let severity = match r.get("severity").as_str() {
        Some("Error") => Severity::Error,
        Some("Info") => Severity::Info,
        _ => Severity::Warning,
    };
    let filename = r.get("filename").as_str().unwrap_or("");
    let mut fixes = Vec::new();
    let text = std::fs::read_to_string(filename).unwrap_or_default();
    for x in r.get("fixes").as_arr() {
        let n = |k: &str| x.get(k).as_u32().unwrap_or(0);
        if text.is_empty() || n("line") == 0 {
            continue;
        }
        fixes.push(crate::fix::Fix {
            path: filename.to_string(),
            start: offset(&text, n("line"), n("column")),
            end: offset(&text, n("endLine").max(n("line")), n("endColumn")),
            text: x.get("replacement").as_str().unwrap_or("").to_string(),
            description: x.get("description").as_str().unwrap_or("").to_string(),
            safety: match x.get("safety").as_str() {
                Some("safe") => crate::fix::FixSafety::Safe,
                Some("unsafe") => crate::fix::FixSafety::Unsafe,
                _ => crate::fix::FixSafety::DisplayOnly,
            },
        });
    }
    let file = file_index(files, filename);
    Some(Finding {
        rule: leak(id),
        name: leak(name),
        severity,
        message: r.get("message").as_str().unwrap_or("").to_string(),
        loc: Loc { file, line: num("line"), column: num("column"), expansion: None },
        related: Vec::new(),
        fixes,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn offsets_count_lines_and_columns_from_one() {
        let t = "ab\ncde\n";
        assert_eq!(offset(t, 1, 1), 0);
        assert_eq!(offset(t, 2, 2), 4);
        assert_eq!(offset(t, 9, 9), t.len() as u32);
    }

    #[test]
    fn reads_a_runner_finding() {
        let mut files = vec![String::new()];
        let r = parse(r#"{"rule_id": "PY-X-001", "rule_name": "", "message": "m", "severity": "Error", "filename": "a.sv", "line": 3, "column": 0}"#).unwrap();
        let f = finding(&r, &mut files).unwrap();
        assert_eq!(
            (f.rule, f.name, f.severity, f.loc.file, f.loc.line, f.loc.column),
            ("PY-X-001", "PY-X-001", Severity::Error, 1, 3, 1)
        );
        assert_eq!(files, ["", "a.sv"]);
    }
}
