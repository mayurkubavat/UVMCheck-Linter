//! Diagnostics and quick fixes from the linter (dv-lint), for the files open
//! in the editor.
//!
//! Linting runs on a thread of its own, so navigation requests are answered
//! while a check is under way. Every edit asks for a check; the thread waits
//! for a pause in typing, then checks the project once with the editor's
//! text standing in for the open files (`svlang::syntax::overlay`) and publishes
//! each open file's diagnostics. What it published is kept, so a code action
//! request is answered from it without linting again.

use std::collections::HashMap;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{channel, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime};

use crate::json::{obj, Json};
use crate::text::{path_to_uri, word_at, LineIndex};

/// Where the server writes messages: shared with the lint thread.
pub type Out = Arc<Mutex<Box<dyn Write + Send>>>;

/// Writes one framed message.
pub fn send(out: &Out, msg: &Json) {
    let body = msg.to_string();
    let mut out = out.lock().unwrap_or_else(|e| e.into_inner());
    let _ = write!(out, "Content-Length: {}\r\n\r\n{}", body.len(), body);
    let _ = out.flush();
}

/// How long typing must pause before a check starts.
const QUIET: Duration = Duration::from_millis(250);

/// A text edit of a quick fix, in LSP positions.
#[derive(Clone)]
struct Edit {
    title: String,
    range: [u32; 4],
    text: String,
    preferred: bool,
}

/// One published diagnostic.
#[derive(Clone)]
struct Diagnostic {
    range: [u32; 4],
    json: Json,
    fixes: Vec<Edit>,
}

#[derive(Default)]
struct State {
    /// Open files, with the URI the editor names each by.
    open: HashMap<PathBuf, String>,
    /// What was last published per open file.
    published: HashMap<PathBuf, Vec<Diagnostic>>,
}

/// The lint side of the server.
pub struct Linter {
    wake: Sender<()>,
    state: Arc<Mutex<State>>,
    out: Out,
}

fn range_json(r: [u32; 4]) -> Json {
    let at = |l: u32, c: u32| obj(vec![("line", l.into()), ("character", c.into())]);
    obj(vec![("start", at(r[0], r[1])), ("end", at(r[2], r[3]))])
}

/// The file of a finding as the editor knows it.
fn canonical(p: &Path) -> PathBuf {
    std::fs::canonicalize(p).unwrap_or_else(|_| p.to_path_buf())
}

/// The text of files, with their line indexes, read once per check.
#[derive(Default)]
struct Texts(HashMap<String, Option<(String, LineIndex)>>);

impl Texts {
    fn get(&mut self, path: &str) -> Option<&(String, LineIndex)> {
        self.0
            .entry(path.to_string())
            .or_insert_with(|| {
                let text = svlang::syntax::overlay::read(Path::new(path)).ok()?;
                let index = LineIndex::new(&text);
                Some((text, index))
            })
            .as_ref()
    }

    /// The leading blanks of line `line` (0-based) of `path`.
    fn indent(&mut self, path: &str, line: u32) -> String {
        let Some((text, _)) = self.get(path) else { return String::new() };
        let l = text.lines().nth(line as usize).unwrap_or("");
        l[..l.len() - l.trim_start().len()].to_string()
    }

    /// The LSP position of byte `offset` of `path`.
    fn position(&mut self, path: &str, offset: u32) -> Option<(u32, u32)> {
        let (text, index) = self.get(path)?;
        Some(index.position(text, offset))
    }

    /// The range of a finding at `line`:`col` (1-based, bytes): the word
    /// there, else one character; the line's code for column 0 (a finding
    /// on a macro call).
    fn range(&mut self, path: &str, line: u32, col: u32) -> [u32; 4] {
        let l = line.saturating_sub(1);
        let Some((text, index)) = self.get(path) else { return [l, 0, l, 1] };
        let start = index.offset(text, l, 0);
        if col == 0 {
            let rest = &text[start as usize..];
            let rest = &rest[..rest.find('\n').unwrap_or(rest.len())];
            let lead = (rest.len() - rest.trim_start().len()) as u32;
            let (_, c0) = index.position(text, start + lead);
            let (_, c1) = index.position(text, start + rest.trim_end().len() as u32);
            return [l, c0, l, c1.max(c0 + 1)];
        }
        let at = (start + col - 1).min(text.len() as u32);
        let end = match word_at(text, at) {
            Some((a, b)) if a as u32 == at => b as u32,
            _ => at + 1,
        };
        let (l0, c0) = index.position(text, at);
        let (l1, c1) = index.position(text, end);
        if l1 != l0 || c1 <= c0 {
            [l0, c0, l0, c0 + 1]
        } else {
            [l0, c0, l1, c1]
        }
    }
}

/// Files with their modification times: what a loaded project depends on.
type Stamps = Vec<(PathBuf, Option<SystemTime>)>;

/// The project of `root`, reloaded when its config or waiver files change.
struct Project {
    root: PathBuf,
    loaded: Option<(Stamps, Result<dv_lint::Workspace, String>)>,
}

impl Project {
    fn stamps(root: &Path, ws: Option<&dv_lint::Workspace>) -> Stamps {
        let mut files: Vec<PathBuf> = Vec::new();
        let base = ws.map_or(root, |w| w.root()).to_path_buf();
        files.extend(ws.and_then(|w| w.config_file()).map(Path::to_path_buf));
        for n in [
            ".uvmcheck/config.py",
            ".uvmcheck/config.json",
            "uvmcheck_config.py",
            ".uvmcheck.py",
            ".uvmcheck.json",
            "uvmcheck.json",
            ".uvmcheck/waivers.json",
            ".uvmcheck/baseline.json",
        ] {
            files.push(base.join(n));
        }
        files.sort();
        files.dedup();
        files.into_iter().map(|f| (f.clone(), std::fs::metadata(&f).and_then(|m| m.modified()).ok())).collect()
    }

    fn workspace(&mut self) -> &Result<dv_lint::Workspace, String> {
        let stale = match &self.loaded {
            Some((stamps, ws)) => *stamps != Self::stamps(&self.root, ws.as_ref().ok()),
            None => true,
        };
        if stale {
            let ws = dv_lint::Workspace::open(&self.root);
            self.loaded = Some((Self::stamps(&self.root, ws.as_ref().ok()), ws));
        }
        &self.loaded.as_ref().expect("just loaded").1
    }
}

/// Checks the project and publishes each open file's diagnostics.
fn check(project: &mut Project, state: &Mutex<State>, out: &Out) {
    let open: HashMap<PathBuf, String> = state.lock().unwrap_or_else(|e| e.into_inner()).open.clone();
    if open.is_empty() {
        return;
    }
    let paths: Vec<PathBuf> = open.keys().cloned().collect();
    let checked = match project.workspace() {
        Ok(ws) => std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| ws.check(&paths)))
            .unwrap_or_else(|_| Err("the linter stopped on an internal error".to_string())),
        Err(e) => Err(e.clone()),
    };
    let checked = match checked {
        Ok(c) => c,
        Err(e) => {
            let params = obj(vec![("type", 2u32.into()), ("message", format!("uvmcheck: {e}").into())]);
            send(
                out,
                &obj(vec![("jsonrpc", "2.0".into()), ("method", "window/logMessage".into()), ("params", params)]),
            );
            return;
        }
    };
    let by_canonical: HashMap<PathBuf, &PathBuf> = open.keys().map(|p| (canonical(p), p)).collect();
    let mut texts = Texts::default();
    let mut found: HashMap<PathBuf, Vec<Diagnostic>> = HashMap::new();
    for f in &checked.findings {
        let (file, line, col) = f.position_in(&checked.files);
        let Some(&path) = by_canonical.get(&canonical(Path::new(file))) else { continue };
        let range = texts.range(file, line, col);
        let related: Vec<Json> = f
            .related
            .iter()
            .filter_map(|(loc, message)| {
                let file = checked.files.get(loc.file as usize).filter(|f| !f.is_empty() && loc.line > 0)?;
                let r = texts.range(file, loc.line, loc.column);
                let location =
                    obj(vec![("uri", path_to_uri(&canonical(Path::new(file))).into()), ("range", range_json(r))]);
                Some(obj(vec![("location", location), ("message", message.as_str().into())]))
            })
            .collect();
        let severity: u32 = match f.severity {
            dv_lint::Severity::Error => 1,
            dv_lint::Severity::Info => 3,
            _ => 2,
        };
        let mut fields = vec![
            ("range", range_json(range)),
            ("severity", severity.into()),
            ("code", f.rule.into()),
            ("source", "uvmcheck".into()),
            ("message", format!("{} ({})", f.message, f.name).into()),
        ];
        if !related.is_empty() {
            fields.push(("relatedInformation", Json::Arr(related)));
        }
        let fixes = f
            .fixes
            .iter()
            .filter(|x| x.safety != dv_lint::fix::FixSafety::DisplayOnly)
            .filter(|x| canonical(Path::new(&x.path)) == canonical(path))
            .filter_map(|x| {
                let (l0, c0) = texts.position(&x.path, x.start)?;
                let (l1, c1) = texts.position(&x.path, x.end)?;
                Some(Edit {
                    title: if x.description.is_empty() { format!("Fix {}", f.rule) } else { x.description.clone() },
                    range: [l0, c0, l1, c1],
                    text: x.text.clone(),
                    preferred: x.safety == dv_lint::fix::FixSafety::Safe,
                })
            })
            .collect();
        let mut fixes: Vec<Edit> = fixes;
        // Any finding but a compile error or a broken waiver can be waived
        // on its line: a comment on the line above, indented like it.
        if !matches!(f.rule, "SV-CMP-001" | "UVM-WV-001") {
            let indent = texts.indent(file, range[0]);
            fixes.push(Edit {
                title: format!("Waive {} on this line", f.rule),
                range: [range[0], 0, range[0], 0],
                text: format!("{indent}// uvmcheck-disable-next-line {}\n", f.rule),
                preferred: false,
            });
        }
        found.entry(path.clone()).or_default().push(Diagnostic { range, json: obj(fields), fixes });
    }
    let mut state = state.lock().unwrap_or_else(|e| e.into_inner());
    for (path, uri) in &open {
        // Closed while the check ran: its diagnostics were cleared already.
        if !state.open.contains_key(path) {
            continue;
        }
        let mut list = found.remove(path).unwrap_or_default();
        list.sort_by_key(|d| (d.range[0], d.range[1]));
        publish(out, uri, &list);
        state.published.insert(path.clone(), list);
    }
}

fn publish(out: &Out, uri: &str, list: &[Diagnostic]) {
    let params =
        obj(vec![("uri", uri.into()), ("diagnostics", Json::Arr(list.iter().map(|d| d.json.clone()).collect()))]);
    send(
        out,
        &obj(vec![("jsonrpc", "2.0".into()), ("method", "textDocument/publishDiagnostics".into()), ("params", params)]),
    );
}

impl Linter {
    /// Starts the lint thread for the project of `root`.
    pub fn start(root: PathBuf, out: Out) -> Linter {
        let (wake, asked) = channel::<()>();
        let state: Arc<Mutex<State>> = Default::default();
        let (thread_state, thread_out) = (state.clone(), out.clone());
        let spawned = std::thread::Builder::new().name("lint".into()).stack_size(256 << 20).spawn(move || {
            let mut project = Project { root, loaded: None };
            while asked.recv().is_ok() {
                // One check for a burst of edits: wait for typing to pause.
                loop {
                    match asked.recv_timeout(QUIET) {
                        Ok(()) => continue,
                        Err(RecvTimeoutError::Timeout) => break,
                        Err(RecvTimeoutError::Disconnected) => return,
                    }
                }
                check(&mut project, &thread_state, &thread_out);
            }
        });
        if let Err(e) = spawned {
            eprintln!("[dv-lsp] no lint thread: {e}");
        }
        Linter { wake, state, out }
    }

    /// Asks for a check of the project (soon, once typing pauses).
    pub fn request(&self) {
        let _ = self.wake.send(());
    }

    /// The editor opened `path` or changed its text.
    pub fn changed(&self, path: &Path, uri: &str, text: &str) {
        svlang::syntax::overlay::set(path, text);
        self.state.lock().unwrap_or_else(|e| e.into_inner()).open.insert(path.to_path_buf(), uri.to_string());
        self.request();
    }

    /// The editor closed `path`: the file on disk counts again, and its
    /// diagnostics go away.
    pub fn closed(&self, path: &Path, uri: &str) {
        svlang::syntax::overlay::clear(path);
        {
            let mut state = self.state.lock().unwrap_or_else(|e| e.into_inner());
            state.open.remove(path);
            state.published.remove(path);
        }
        publish(&self.out, uri, &[]);
        self.request();
    }

    /// Quick fixes for the diagnostics of `path` that touch lines
    /// `first..=last`.
    pub fn code_actions(&self, path: &Path, uri: &str, first: u32, last: u32) -> Json {
        let state = self.state.lock().unwrap_or_else(|e| e.into_inner());
        let mut actions = Vec::new();
        for d in state.published.get(path).map_or(&[][..], |l| &l[..]) {
            if d.range[2] < first || d.range[0] > last {
                continue;
            }
            for fix in &d.fixes {
                let edit = obj(vec![("range", range_json(fix.range)), ("newText", fix.text.as_str().into())]);
                let changes = obj(vec![(uri, Json::Arr(vec![edit]))]);
                actions.push(obj(vec![
                    ("title", fix.title.as_str().into()),
                    ("kind", "quickfix".into()),
                    ("diagnostics", Json::Arr(vec![d.json.clone()])),
                    ("isPreferred", fix.preferred.into()),
                    ("edit", obj(vec![("changes", changes)])),
                ]));
            }
        }
        Json::Arr(actions)
    }
}
