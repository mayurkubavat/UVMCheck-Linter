//! SystemVerilog preprocessor (IEEE 1800-2023 clause 22) on top of sv-lexer:
//! `define/`undef/`undefineall, `ifdef/`ifndef/`elsif/`else/`endif (with
//! 2023 expressions), `include with include directories, `__FILE__ and
//! `__LINE__, and macro expansion with arguments, defaults, `" `\`" and ``.
//!
//! The output is the token stream a parser reads: no trivia and no
//! directives, each token with the place in the source it came from (for a
//! token made by a macro, the macro use).
//!
//! ```
//! let mut pp = sv_preproc::Preprocessor::new(sv_preproc::Options::default());
//! let out = pp.run_source("`define W 8\nlogic [`W-1:0] x;\n", "t.sv");
//! let text: Vec<_> = out.tokens.iter().map(|t| &*t.text).collect();
//! assert_eq!(text, ["logic", "[", "8", "-", "1", ":", "0", "]", "x", ";"]);
//! ```

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::rc::Rc;

use sv_lexer::Kind;

#[derive(Clone, Debug, Default)]
pub struct Options {
    /// Searched, in order, for `include files after the including file's directory.
    pub include_dirs: Vec<PathBuf>,
    /// `NAME` or `NAME=VALUE`, as +define+ gives them.
    pub defines: Vec<String>,
    /// Deepest `include nesting before an error.
    pub max_include_depth: usize,
}

/// Where a token came from: a file (index into `Output::files`) and a byte offset.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Origin {
    pub file: u32,
    pub offset: u32,
}

#[derive(Clone, Debug)]
pub struct PpToken {
    pub kind: Kind,
    pub text: Rc<str>,
    pub origin: Origin,
    /// Made or rearranged by a macro expansion.
    pub from_macro: bool,
    /// Where the token's text was written: inside a `define body or a
    /// macro argument for expanded tokens (slang's "fully original"
    /// location), else the same as `origin`.
    pub written: Origin,
}

#[derive(Clone, Debug)]
pub struct Diagnostic {
    pub origin: Origin,
    pub message: String,
}

#[derive(Debug, Default)]
pub struct Output {
    pub tokens: Vec<PpToken>,
    pub files: Vec<PathBuf>,
    /// The text of each file in `files`, as read.
    pub sources: Vec<Rc<str>>,
    pub diagnostics: Vec<Diagnostic>,
}

/// A token inside the preprocessor: no trivia, but whether trivia came
/// before it (stringification keeps single spaces) and whether a line break
/// did (directives end at the end of their line).
#[derive(Clone, Debug)]
struct T {
    kind: Kind,
    text: Rc<str>,
    space_before: bool,
    /// The whitespace (not comments or line breaks) before the token, as
    /// written; stringification keeps it.
    ws: Rc<str>,
    /// How much of `ws` came before a line continuation. Slang turns a
    /// continuation into trivia for the next token produced, so inside a
    /// define this part follows `" into the stringified text.
    lc_len: u32,
    newline_before: bool,
    origin: Origin,
    from_macro: bool,
    /// Where the text was written (a define body or a macro argument);
    /// None for text a macro made (pasting, stringifying, the command line).
    written: Option<Origin>,
}

#[derive(Clone, Debug)]
struct Macro {
    /// None: an object-like macro; Some: the formal arguments and their defaults.
    params: Option<Vec<(String, Option<Vec<T>>)>>,
    body: Vec<T>,
}

struct Frame {
    toks: Vec<T>,
    idx: usize,
    /// The macro this frame expands, to stop recursion; None for a file.
    macro_name: Option<Rc<str>>,
    /// The file this frame reads, for `include lookup and `__FILE__.
    file: Option<u32>,
}

#[derive(Clone, Copy)]
struct Cond {
    /// Some branch of this `ifdef chain has been taken.
    taken: bool,
}

pub struct Preprocessor {
    opts: Options,
    macros: HashMap<String, Rc<Macro>>,
    frames: Vec<Frame>,
    conds: Vec<Cond>,
    out: Output,
    paths: Vec<PathBuf>,
    sources: Vec<Rc<str>>,
    /// `begin_keywords versions in force, innermost last.
    keyword_versions: Vec<sv_lexer::KeywordVersion>,
    /// Inside a module, interface, program or package (some directives
    /// are not allowed there).
    in_design_element: bool,
    /// Text of the last token handed out, for `extern module` and
    /// `interface class`.
    last_text: Rc<str>,
    /// The last token opened a design element.
    just_opened: bool,
}

const STANDARD_MACROS: &[(&str, &str)] = &[
    ("SV_COV_START", "0"),
    ("SV_COV_STOP", "1"),
    ("SV_COV_RESET", "2"),
    ("SV_COV_CHECK", "3"),
    ("SV_COV_MODULE", "10"),
    ("SV_COV_HIER", "11"),
    ("SV_COV_ASSERTION", "20"),
    ("SV_COV_FSM_STATE", "21"),
    ("SV_COV_STATEMENT", "22"),
    ("SV_COV_TOGGLE", "23"),
    ("SV_COV_OVERFLOW", "-2"),
    ("SV_COV_ERROR", "-1"),
    ("SV_COV_NOCOV", "0"),
    ("SV_COV_OK", "1"),
    ("SV_COV_PARTIAL", "2"),
];

/// Directives that take the rest of their line and leave nothing behind.
const LINE_DIRECTIVES: &[&str] = &[
    "timescale",
    "default_nettype",
    "resetall",
    "celldefine",
    "endcelldefine",
    "unconnected_drive",
    "nounconnected_drive",
    "pragma",
    "line",
    "default_decay_time",
    "default_trireg_strength",
    "delay_mode_distributed",
    "delay_mode_path",
    "delay_mode_unit",
    "delay_mode_zero",
    "protect",
    "endprotect",
    "protected",
    "endprotected",
];

fn empty() -> Rc<str> {
    thread_local!(static EMPTY: Rc<str> = Rc::from(""));
    EMPTY.with(|e| e.clone())
}

fn lex_to_t(src: &str, file: u32, from_macro: bool, origin_override: Option<Origin>) -> Vec<T> {
    let mut out = Vec::new();
    let (mut space, mut newline) = (false, true);
    let mut continuation = false;
    let mut ws = String::new();
    let mut lc_len = 0usize;
    for tok in sv_lexer::lex(src) {
        match tok.kind {
            Kind::Whitespace => {
                ws.push_str(tok.text(src));
                space = true;
            }
            Kind::BlockComment => space = true,
            // `// note \` inside a `define still continues the macro.
            Kind::LineComment => {
                continuation = tok.text(src).ends_with('\\');
                space = true;
            }
            Kind::LineContinuation => {
                lc_len = ws.len();
                continuation = true;
                space = true;
            }
            Kind::Newline => {
                // A backslash-newline continues the line (a `define body).
                if !continuation {
                    newline = true;
                }
                continuation = false;
                space = true;
            }
            _ => {
                continuation = false;
                out.push(T {
                    kind: tok.kind,
                    text: Rc::from(tok.text(src)),
                    space_before: space,
                    lc_len: std::mem::take(&mut lc_len) as u32,
                    ws: if ws.is_empty() { empty() } else { Rc::from(std::mem::take(&mut ws)) },
                    newline_before: newline,
                    origin: origin_override.unwrap_or(Origin { file, offset: tok.start }),
                    from_macro,
                    written: (!from_macro).then_some(Origin { file, offset: tok.start }),
                });
                space = false;
                newline = false;
            }
        }
    }
    out
}

impl Preprocessor {
    pub fn new(opts: Options) -> Self {
        let mut pp = Preprocessor {
            opts,
            macros: HashMap::new(),
            frames: Vec::new(),
            conds: Vec::new(),
            out: Output::default(),
            paths: Vec::new(),
            sources: Vec::new(),
            keyword_versions: Vec::new(),
            in_design_element: false,
            last_text: empty(),
            just_opened: false,
        };
        if pp.opts.max_include_depth == 0 {
            pp.opts.max_include_depth = 1024;
        }
        pp.reset_macros();
        pp
    }

    fn reset_macros(&mut self) {
        self.macros.clear();
        for (name, value) in STANDARD_MACROS {
            self.define_text(name, value);
        }
        for d in self.opts.defines.clone() {
            let (name, value) = d.split_once('=').unwrap_or((&d, "1"));
            self.define_text(name, value);
        }
    }

    fn define_text(&mut self, name: &str, value: &str) {
        let body = lex_to_t(value, 0, true, None);
        self.macros.insert(name.to_string(), Rc::new(Macro { params: None, body }));
    }

    pub fn is_defined(&self, name: &str) -> bool {
        self.macros.contains_key(name)
    }

    /// Preprocesses one file (and what it includes).
    pub fn run_file(&mut self, path: &Path) -> Output {
        match std::fs::read(path) {
            Ok(bytes) => {
                let src = String::from_utf8_lossy(&bytes).into_owned();
                self.run_source(&src, &path.to_string_lossy())
            }
            Err(e) => {
                let mut out = std::mem::take(&mut self.out);
                out.diagnostics.push(Diagnostic {
                    origin: Origin::default(),
                    message: format!("cannot read {}: {e}", path.display()),
                });
                out
            }
        }
    }

    /// Preprocesses `src` as if read from a file called `name`. Macros
    /// defined so far stay defined, as in a single compilation unit.
    pub fn run_source(&mut self, src: &str, name: &str) -> Output {
        let id = self.add_file(PathBuf::from(name), src);
        self.push_file(id);
        while let Some(t) = self.next() {
            let written = t.written.unwrap_or(t.origin);
            self.out.tokens.push(PpToken { kind: t.kind, text: t.text, origin: t.origin, from_macro: t.from_macro, written });
        }
        if !self.conds.is_empty() {
            self.diag(Origin { file: id, offset: src.len() as u32 }, "missing `endif".into());
            self.conds.clear();
        }
        let mut out = std::mem::take(&mut self.out);
        out.files = self.paths.clone();
        out.sources = self.sources.clone();
        out
    }

    fn add_file(&mut self, path: PathBuf, src: &str) -> u32 {
        self.paths.push(path);
        self.sources.push(Rc::from(src));
        (self.paths.len() - 1) as u32
    }

    fn push_file(&mut self, id: u32) {
        let src = self.sources[id as usize].clone();
        let toks = lex_to_t(&src, id, false, None);
        self.frames.push(Frame { toks, idx: 0, macro_name: None, file: Some(id) });
    }

    fn diag(&mut self, origin: Origin, message: String) {
        self.out.diagnostics.push(Diagnostic { origin, message });
    }

    fn current_file(&self) -> Option<u32> {
        self.frames.iter().rev().find_map(|f| f.file)
    }

    // ---- raw token access -------------------------------------------------

    /// Next token from the stack, popping finished frames.
    fn raw_next(&mut self) -> Option<T> {
        loop {
            let f = self.frames.last_mut()?;
            if f.idx < f.toks.len() {
                f.idx += 1;
                return Some(f.toks[f.idx - 1].clone());
            }
            self.frames.pop();
        }
    }

    fn raw_peek(&self) -> Option<&T> {
        self.frames.iter().rev().find_map(|f| f.toks.get(f.idx))
    }

    /// The next token if it is on the same line in the same frame.
    fn line_next(&mut self) -> Option<T> {
        let f = self.frames.last_mut()?;
        let t = f.toks.get(f.idx)?;
        if t.newline_before {
            return None;
        }
        f.idx += 1;
        Some(t.clone())
    }

    fn line_peek(&self) -> Option<&T> {
        let f = self.frames.last()?;
        f.toks.get(f.idx).filter(|t| !t.newline_before)
    }

    fn rest_of_line(&mut self) -> Vec<T> {
        let mut v = Vec::new();
        while let Some(t) = self.line_next() {
            v.push(t);
        }
        v
    }

    // ---- main loop --------------------------------------------------------

    fn next(&mut self) -> Option<T> {
        loop {
            let mut t = self.raw_next()?;
            if t.kind != Kind::Directive {
                if t.kind == Kind::Keyword {
                    if let Some(&v) = self.keyword_versions.last() {
                        if !sv_lexer::is_keyword_in(v, &t.text) {
                            // Not reserved under the `begin_keywords in force.
                            t.kind = Kind::Identifier;
                        }
                    }
                    self.track_design_element(&t);
                }
                self.last_text = t.text.clone();
                return Some(t);
            }
            let name = &t.text[1..];
            match name {
                "define" => self.directive_define(&t),
                "undef" => {
                    if let Some(n) = self.line_next() {
                        self.macros.remove(&*n.text);
                    }
                }
                "undefineall" => self.reset_macros(),
                "ifdef" | "ifndef" => {
                    let cond = self.condition(&t);
                    let take = if name == "ifdef" { cond } else { !cond };
                    self.conds.push(Cond { taken: take });
                    if !take {
                        self.skip_branch();
                    }
                }
                "elsif" | "else" => {
                    // We were in a taken branch: skip to `endif.
                    if name == "elsif" {
                        self.condition(&t);
                    }
                    if self.conds.is_empty() {
                        self.diag(t.origin, format!("`{name} without `ifdef"));
                    } else {
                        self.skip_to_endif();
                    }
                }
                "endif" => {
                    if self.conds.pop().is_none() {
                        self.diag(t.origin, "`endif without `ifdef".into());
                    }
                }
                "include" => self.directive_include(&t),
                "__FILE__" => {
                    let file = self.current_file().map(|f| self.paths[f as usize].to_string_lossy().into_owned());
                    let text = format!("\"{}\"", file.unwrap_or_default());
                    return Some(T { kind: Kind::StringLiteral, text: Rc::from(text), from_macro: true, ..t });
                }
                "__LINE__" => {
                    let line = self.line_of(t.origin);
                    return Some(T { kind: Kind::IntegerLiteral, text: Rc::from(line.to_string()), from_macro: true, ..t });
                }
                "begin_keywords" => {
                    let spec = self.line_next();
                    let v = spec.as_ref().and_then(|s| {
                        sv_lexer::KeywordVersion::from_specifier(s.text.trim_matches('"'))
                    });
                    match v {
                        Some(v) => self.keyword_versions.push(v),
                        None => self.diag(t.origin, "unknown `begin_keywords version".into()),
                    }
                    self.rest_of_line();
                }
                "end_keywords" => {
                    if self.keyword_versions.pop().is_none() {
                        self.diag(t.origin, "`end_keywords without `begin_keywords".into());
                    }
                    self.rest_of_line();
                }
                "resetall" | "nounconnected_drive" | "default_nettype" | "unconnected_drive"
                    if self.in_design_element =>
                {
                    self.diag(t.origin, format!("`{name} inside a design element"));
                    self.directive_args(&t, name);
                }
                _ if LINE_DIRECTIVES.contains(&name) => self.directive_args(&t, name),
                _ => {
                    if let Some(m) = self.macros.get(name).cloned() {
                        self.expand(&t, name, &m);
                    } else {
                        self.diag(t.origin, format!("unknown macro or directive `{name}"));
                        // Like Slang: a parenthesis next means a function-like
                        // use, so its argument list is skipped too.
                        if self.raw_peek().is_some_and(|n| &*n.text == "(") {
                            self.actual_args(&t, name);
                        }
                    }
                }
            }
        }
    }

    /// Module, interface, program and package bodies, at the outer level
    /// only (nested modules are rare and only make this more lenient).
    fn track_design_element(&mut self, t: &T) {
        let just_opened = std::mem::take(&mut self.just_opened);
        match &*t.text {
            "module" | "macromodule" | "program" | "package" | "interface"
                if !self.in_design_element && !matches!(&*self.last_text, "extern" | "virtual") =>
            {
                self.in_design_element = true;
                self.just_opened = true;
            }
            "class" if just_opened && &*self.last_text == "interface" => self.in_design_element = false,
            "endmodule" | "endprogram" | "endpackage" | "endinterface" => self.in_design_element = false,
            _ => {}
        }
    }

    /// Arguments of the directives that are checked and dropped. Those
    /// without arguments leave the rest of the line to the parser, as
    /// Slang does.
    fn directive_args(&mut self, d: &T, name: &str) {
        match name {
            "resetall" | "celldefine" | "endcelldefine" | "nounconnected_drive" | "delay_mode_distributed"
            | "delay_mode_path" | "delay_mode_unit" | "delay_mode_zero" => {}
            "default_nettype" | "default_decay_time" | "default_trireg_strength" => {
                if self.line_next().is_none() {
                    self.diag(d.origin, format!("`{name} needs a value"));
                }
            }
            "unconnected_drive" => {
                if !self.line_next().is_some_and(|t| matches!(&*t.text, "pull0" | "pull1")) {
                    self.diag(d.origin, "`unconnected_drive needs pull0 or pull1".into());
                }
            }
            "pragma" => {
                let rest = self.rest_of_line();
                if !rest.first().is_some_and(|t| matches!(t.kind, Kind::Identifier | Kind::Keyword)) {
                    self.diag(d.origin, "`pragma needs a name".into());
                }
            }
            "line" => {
                let rest = self.rest_of_line();
                let ok = rest.len() == 3
                    && rest[0].kind == Kind::IntegerLiteral
                    && rest[1].kind == Kind::StringLiteral
                    && matches!(&*rest[2].text, "0" | "1" | "2");
                if !ok {
                    self.diag(d.origin, "`line needs a line number, a \"file\" and a level 0, 1 or 2".into());
                }
            }
            "timescale" => {
                let rest = self.rest_of_line();
                if rest.iter().all(|t| t.kind != Kind::Directive) {
                    let spec: String = rest.iter().map(|t| &*t.text).collect();
                    if let Some(msg) = timescale_error(&spec) {
                        self.diag(d.origin, msg);
                    }
                }
            }
            _ => {
                self.rest_of_line();
            }
        }
    }

    fn line_of(&self, o: Origin) -> usize {
        let src = &self.sources[o.file as usize];
        1 + src.as_bytes()[..(o.offset as usize).min(src.len())].iter().filter(|&&b| b == b'\n').count()
    }

    // ---- `define ----------------------------------------------------------

    fn directive_define(&mut self, d: &T) {
        let Some(name) = self.line_next() else {
            self.diag(d.origin, "`define without a name".into());
            return;
        };
        if is_directive_name(&name.text) {
            self.diag(name.origin, format!("`{} is a directive and cannot be a macro name", name.text));
        }
        let mut params = None;
        if self.line_peek().is_some_and(|t| &*t.text == "(" && !t.space_before) {
            self.line_next();
            let mut list = Vec::new();
            loop {
                let Some(p) = self.line_next() else { break };
                if &*p.text == ")" {
                    break;
                }
                if &*p.text == "," {
                    continue;
                }
                let mut default = None;
                if self.line_peek().is_some_and(|t| &*t.text == "=") {
                    self.line_next();
                    let mut toks = Vec::new();
                    let mut depth = 0i32;
                    while let Some(t) = self.line_peek() {
                        let s = &*t.text;
                        if depth == 0 && (s == "," || s == ")") {
                            break;
                        }
                        if matches!(s, "(" | "[" | "{" | "'{") {
                            depth += 1;
                        } else if matches!(s, ")" | "]" | "}") {
                            depth -= 1;
                        }
                        toks.push(self.line_next().unwrap());
                    }
                    default = Some(toks);
                }
                list.push((p.text.to_string(), default));
            }
            params = Some(list);
        }
        let body = self.rest_of_line();
        self.macros.insert(name.text.to_string(), Rc::new(Macro { params, body }));
    }

    // ---- conditionals -----------------------------------------------------

    /// Reads the condition after `ifdef/`ifndef/`elsif: a name or, from
    /// 1800-2023, a parenthesized expression of names, !, &&, ||, -> and <->.
    fn condition(&mut self, d: &T) -> bool {
        let Some(first) = self.line_next() else {
            self.diag(d.origin, "missing macro name".into());
            return false;
        };
        if &*first.text != "(" {
            return self.is_defined(&first.text);
        }
        // Gather to the matching ')' and evaluate.
        let mut toks = vec![first];
        let mut depth = 1;
        while depth > 0 {
            let Some(t) = self.line_next() else { break };
            match &*t.text {
                "(" => depth += 1,
                ")" => depth -= 1,
                _ => {}
            }
            toks.push(t);
        }
        let mut i = 0;
        self.cond_expr(&toks, &mut i)
    }

    fn cond_expr(&self, t: &[T], i: &mut usize) -> bool {
        let mut v = self.cond_and(t, i);
        while *i < t.len() && matches!(&*t[*i].text, "||" | "->" | "<->") {
            let op = t[*i].text.clone();
            *i += 1;
            let r = self.cond_and(t, i);
            v = match &*op {
                "||" => v || r,
                "->" => !v || r,
                _ => v == r,
            };
        }
        v
    }

    fn cond_and(&self, t: &[T], i: &mut usize) -> bool {
        let mut v = self.cond_unary(t, i);
        while *i < t.len() && &*t[*i].text == "&&" {
            *i += 1;
            let r = self.cond_unary(t, i);
            v = v && r;
        }
        v
    }

    fn cond_unary(&self, t: &[T], i: &mut usize) -> bool {
        let Some(tok) = t.get(*i) else { return false };
        *i += 1;
        match &*tok.text {
            "!" => !self.cond_unary(t, i),
            "(" => {
                let v = self.cond_expr(t, i);
                if t.get(*i).is_some_and(|x| &*x.text == ")") {
                    *i += 1;
                }
                v
            }
            name => self.is_defined(name),
        }
    }

    /// Skips the rest of a branch not taken; stops after the `elsif/`else
    /// that takes over, or after the `endif.
    fn skip_branch(&mut self) {
        let mut depth = 0;
        while let Some(t) = self.frame_next() {
            if t.kind != Kind::Directive {
                continue;
            }
            match &t.text[1..] {
                "ifdef" | "ifndef" => depth += 1,
                "endif" if depth > 0 => depth -= 1,
                "endif" => {
                    self.conds.pop();
                    return;
                }
                "elsif" if depth == 0 => {
                    let taken = self.conds.last().map_or(true, |c| c.taken);
                    let cond = self.condition(&t);
                    if !taken && cond {
                        if let Some(c) = self.conds.last_mut() {
                            c.taken = true;
                        }
                        return;
                    }
                }
                "else" if depth == 0 => {
                    let taken = self.conds.last().map_or(true, |c| c.taken);
                    if !taken {
                        if let Some(c) = self.conds.last_mut() {
                            c.taken = true;
                        }
                        return;
                    }
                }
                _ => {}
            }
        }
    }

    fn skip_to_endif(&mut self) {
        if let Some(c) = self.conds.last_mut() {
            c.taken = true;
        }
        self.skip_branch();
    }

    /// Next token of the current frame only (conditionals do not cross files).
    fn frame_next(&mut self) -> Option<T> {
        let f = self.frames.last_mut()?;
        let t = f.toks.get(f.idx)?.clone();
        f.idx += 1;
        Some(t)
    }

    // ---- `include ---------------------------------------------------------

    fn directive_include(&mut self, d: &T) {
        let Some(first) = self.line_next() else {
            self.diag(d.origin, "`include without a file name".into());
            return;
        };
        let (name, angle) = match first.kind {
            Kind::StringLiteral => (first.text[1..first.text.len() - 1].to_string(), false),
            _ if &*first.text == "<" => {
                let mut s = String::new();
                while let Some(t) = self.line_next() {
                    if &*t.text == ">" {
                        break;
                    }
                    if t.space_before && !s.is_empty() {
                        s.push(' ');
                    }
                    s.push_str(&t.text);
                }
                (s, true)
            }
            Kind::Directive => {
                // `include `FILE: expand the macro and take the string it gives.
                let name = first.text[1..].to_string();
                let Some(m) = self.macros.get(&name).cloned() else {
                    self.diag(first.origin, format!("unknown macro `{name} in `include"));
                    return;
                };
                let depth = self.frames.len();
                self.expand(&first, &name, &m);
                let mut text = None;
                while self.frames.len() > depth {
                    if let Some(t) = self.raw_next() {
                        if t.kind == Kind::StringLiteral && text.is_none() {
                            text = Some(t.text[1..t.text.len() - 1].to_string());
                        }
                    }
                }
                match text {
                    Some(s) => (s, false),
                    None => {
                        self.diag(first.origin, "`include macro gives no file name".into());
                        return;
                    }
                }
            }
            _ => {
                self.diag(first.origin, "`include needs \"file\" or <file>".into());
                return;
            }
        };
        if self.frames.iter().filter(|f| f.file.is_some()).count() >= self.opts.max_include_depth {
            self.diag(d.origin, "`include nested too deeply".into());
            return;
        }
        match self.resolve_include(&name, angle) {
            Some(path) => match std::fs::read(&path) {
                Ok(bytes) => {
                    let src = String::from_utf8_lossy(&bytes).into_owned();
                    let id = self.add_file(path, &src);
                    self.push_file(id);
                }
                Err(e) => self.diag(d.origin, format!("cannot read {}: {e}", path.display())),
            },
            None => self.diag(d.origin, format!("cannot find include file \"{name}\"")),
        }
    }

    fn resolve_include(&self, name: &str, angle: bool) -> Option<PathBuf> {
        let p = Path::new(name);
        if p.is_absolute() {
            return includable(p).then(|| p.to_path_buf());
        }
        if !angle {
            if let Some(f) = self.current_file() {
                if let Some(dir) = self.paths[f as usize].parent() {
                    let c = dir.join(p);
                    if includable(&c) {
                        return Some(c);
                    }
                }
            }
        }
        self.opts.include_dirs.iter().map(|d| d.join(p)).find(|c| includable(c))
    }

    // ---- macro expansion --------------------------------------------------

    fn expand(&mut self, use_tok: &T, name: &str, m: &Macro) {
        if self.frames.iter().any(|f| f.macro_name.as_deref() == Some(name)) {
            self.diag(use_tok.origin, format!("recursive use of macro `{name}"));
            return;
        }
        let args = match &m.params {
            None => Vec::new(),
            Some(params) => {
                let Some(actuals) = self.actual_args(use_tok, name) else { return };
                let mut out = Vec::with_capacity(params.len());
                for (k, (_, default)) in params.iter().enumerate() {
                    match actuals.get(k) {
                        Some(a) if !a.is_empty() => out.push(a.clone()),
                        _ => match default {
                            Some(d) => out.push(d.clone()),
                            None if actuals.get(k).is_some() => out.push(Vec::new()),
                            None => {
                                self.diag(use_tok.origin, format!("too few arguments to macro `{name}"));
                                out.push(Vec::new());
                            }
                        },
                    }
                }
                if actuals.len() > params.len() && !(params.is_empty() && actuals.len() == 1 && actuals[0].is_empty()) {
                    self.diag(use_tok.origin, format!("too many arguments to macro `{name}"));
                }
                out
            }
        };
        // Macros used inside an argument are expanded before substitution.
        let args: Vec<Vec<T>> = args
            .into_iter()
            .map(|a| if a.iter().any(|t| t.kind == Kind::Directive) { self.expand_list(a, name) } else { a })
            .collect();
        let names: Vec<&str> = m.params.as_ref().map(|p| p.iter().map(|(n, _)| n.as_str()).collect()).unwrap_or_default();
        let mut toks = self.substitute(use_tok, &m.body, &names, &args);
        // The expansion's first token takes the use's spacing when the use is
        // itself inside an expansion, none at the top level.
        let nested = use_tok.from_macro;
        if let Some(first) = toks.first_mut() {
            first.ws = if nested { use_tok.ws.clone() } else { empty() };
            first.space_before = nested && use_tok.space_before;
        }
        self.frames.push(Frame { toks, idx: 0, macro_name: Some(Rc::from(name)), file: None });
    }

    /// Fully expands a token list on its own (a macro argument), keeping
    /// the recursion guard of the expansions in progress.
    fn expand_list(&mut self, toks: Vec<T>, outer: &str) -> Vec<T> {
        let saved = std::mem::take(&mut self.frames);
        for f in saved.iter().filter(|f| f.macro_name.is_some()) {
            self.frames.push(Frame { toks: Vec::new(), idx: 0, macro_name: f.macro_name.clone(), file: None });
        }
        self.frames.push(Frame { toks: Vec::new(), idx: 0, macro_name: Some(Rc::from(outer)), file: None });
        let file = saved.iter().rev().find_map(|f| f.file);
        self.frames.push(Frame { toks, idx: 0, macro_name: None, file });
        let mut out = Vec::new();
        while let Some(t) = self.next() {
            out.push(t);
        }
        self.frames = saved;
        out
    }

    /// Reads `( a, b, ... )` after a macro use: arguments split at top-level commas.
    fn actual_args(&mut self, use_tok: &T, name: &str) -> Option<Vec<Vec<T>>> {
        if !self.raw_peek().is_some_and(|t| &*t.text == "(") {
            self.diag(use_tok.origin, format!("macro `{name} needs arguments"));
            return None;
        }
        self.raw_next();
        let mut args = vec![Vec::new()];
        let mut depth = 0i32;
        loop {
            let Some(t) = self.raw_next() else {
                self.diag(use_tok.origin, format!("unterminated arguments to macro `{name}"));
                return None;
            };
            match &*t.text {
                ")" if depth == 0 => break,
                "," if depth == 0 => {
                    args.push(Vec::new());
                    continue;
                }
                "(" | "[" | "{" | "'{" => depth += 1,
                ")" | "]" | "}" => depth -= 1,
                _ => {}
            }
            args.last_mut().unwrap().push(t);
        }
        Some(args)
    }

    fn substitute(&self, use_tok: &T, body: &[T], names: &[&str], args: &[Vec<T>]) -> Vec<T> {
        let origin = use_tok.origin;
        let mark = |t: &T, space: bool| T { origin, from_macro: true, space_before: space, newline_before: false, ..t.clone() };
        let param = |s: &str| names.iter().position(|n| *n == s);
        let mut out: Vec<T> = Vec::new();
        let mut i = 0;
        let mut carry: Option<(bool, Rc<str>)> = None;
        while i < body.len() {
            let t = &body[i];
            let before = out.len();
            let mut pending: Option<(bool, Rc<str>)> = None;
            match t.kind {
                Kind::MacroQuote => {
                    // `" ... `": one string literal, arguments substituted.
                    // Like Slang, each token brings its own raw whitespace,
                    // and a substituted argument's first token takes the
                    // formal parameter's whitespace.
                    let mut s = String::from("\"");
                    let push = |s: &mut String, ws: &str, a: &T| {
                        s.push_str(ws);
                        match a.kind {
                            Kind::MacroEscapedQuote => s.push_str("\\\""),
                            Kind::StringLiteral if a.text.starts_with("\"\"\"") && a.text.len() >= 6 => {
                                s.push_str("\\\"\\\"\\\"");
                                s.push_str(&a.text[3..a.text.len() - 3]);
                                s.push_str("\\\"\\\"\\\"");
                            }
                            Kind::StringLiteral if a.text.len() >= 2 => {
                                s.push_str("\\\"");
                                s.push_str(&a.text[1..a.text.len() - 1]);
                                s.push_str("\\\"");
                            }
                            _ => s.push_str(&a.text),
                        }
                    };
                    s.push_str(&t.ws[..t.lc_len as usize]);
                    let mut j = i + 1;
                    while j < body.len() && body[j].kind != Kind::MacroQuote {
                        let b = &body[j];
                        match param(&b.text) {
                            Some(k) if b.kind == Kind::Identifier || b.kind == Kind::Keyword => {
                                if args[k].is_empty() {
                                    s.push_str(&b.ws);
                                }
                                for (n, a) in args[k].iter().enumerate() {
                                    push(&mut s, if n == 0 { &b.ws } else { &a.ws }, a);
                                }
                            }
                            _ if b.kind == Kind::MacroPaste => {}
                            _ => push(&mut s, &b.ws, b),
                        }
                        j += 1;
                    }
                    s.push('"');
                    out.push(T {
                        kind: Kind::StringLiteral,
                        text: Rc::from(s),
                        space_before: t.space_before,
                        ws: t.ws.clone(),
                        lc_len: t.lc_len,
                        newline_before: false,
                        origin,
                        from_macro: true,
                        written: None,
                    });
                    i = j + 1;
                    continue;
                }
                Kind::MacroPaste => {
                    // a``b: glue the last token out with the next one in.
                    let mut next: Vec<T> = Vec::new();
                    if let Some(b) = body.get(i + 1) {
                        match param(&b.text) {
                            Some(k) => next.extend(args[k].iter().cloned()),
                            None => next.push(b.clone()),
                        }
                    }
                    let left = out.pop();
                    let mut glued = left.as_ref().map(|l| l.text.to_string()).unwrap_or_default();
                    let space = left.as_ref().map_or(false, |l| l.space_before);
                    if let Some(first) = next.first() {
                        glued.push_str(&first.text);
                    }
                    let mut re = lex_to_t(&glued, origin.file, true, Some(origin));
                    // Placed where the left half was written, as slang does.
                    let written = left.as_ref().and_then(|l| l.written).or_else(|| next.first().and_then(|n| n.written));
                    for (n, r) in re.iter_mut().enumerate() {
                        r.written = written;
                        if n == 0 {
                            r.space_before = space;
                        }
                        r.newline_before = false;
                    }
                    out.extend(re);
                    out.extend(next.iter().skip(1).map(|a| mark(a, a.space_before)));
                    i += 2;
                    continue;
                }
                Kind::MacroEscapedQuote => out.push(T { kind: Kind::Punct, text: Rc::from("\""), ..mark(t, t.space_before) }),
                _ => match param(&t.text) {
                    Some(k) if t.kind == Kind::Identifier || t.kind == Kind::Keyword => {
                        if args[k].is_empty() {
                            // An empty argument hands its whitespace on.
                            pending = Some((t.space_before, t.ws.clone()));
                        }
                        for (n, a) in args[k].iter().enumerate() {
                            let mut m = mark(a, if n == 0 { t.space_before } else { a.space_before });
                            if n == 0 {
                                m.ws = t.ws.clone();
                            }
                            out.push(m);
                        }
                    }
                    _ => out.push(mark(t, t.space_before)),
                },
            }
            if out.len() > before {
                if let Some((sp, ws)) = carry.take() {
                    out[before].space_before = sp;
                    out[before].ws = ws;
                }
            }
            if pending.is_some() {
                carry = pending.take();
            }
            i += 1;
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pp(src: &str) -> Vec<String> {
        let mut p = Preprocessor::new(Options::default());
        let out = p.run_source(src, "t.sv");
        assert!(out.diagnostics.is_empty(), "{:?}", out.diagnostics);
        out.tokens.iter().map(|t| t.text.to_string()).collect()
    }

    #[test]
    fn object_and_function_macros() {
        assert_eq!(pp("`define A 1\n`define F(x, y=2) (x+y)\na=`A; b=`F(3); c=`F(4,5);"), [
            "a", "=", "1", ";", "b", "=", "(", "3", "+", "2", ")", ";", "c", "=", "(", "4", "+", "5", ")", ";"
        ]);
    }

    #[test]
    fn stringify_paste_and_nesting() {
        let src = "`define S(x) `\"x is `\\`\"q`\\`\"`\"\n`define CAT(a,b) a``_``b\n`define N `CAT(m, n)\n`S(foo) `N";
        assert_eq!(pp(src), ["\"foo is \\\"q\\\"\"", "m_n"]);
    }

    #[test]
    fn stringify_spacing_matches_slang() {
        // Whitespace before a line continuation lands inside the string;
        // a nested string literal is escaped; an argument takes the
        // formal parameter's spacing.
        let src = "`define N(T) h(a, \\\n     `\"T``(s)`\");\n`N(q)\n`define S(x) `\"x  \"y\"`\"\n`S( z)";
        assert_eq!(pp(src), ["h", "(", "a", ",", "\" q(s)\"", ")", ";", "\"z  \\\"y\\\"\""]);
    }

    #[test]
    fn conditionals() {
        let src = "`define A\n`ifdef A a `elsif B b `else c `endif\n`ifndef A d `elsif A e `endif\n`ifdef (A && !B) f `endif\n`ifdef B `ifdef A g `endif `else h `endif";
        assert_eq!(pp(src), ["a", "e", "f", "h"]);
    }

    #[test]
    fn multi_line_define_and_undef() {
        assert_eq!(pp("`define M(a) \\\n  begin \\\n    a; \\\n  end\n`M(x)\n`undef M\n`ifdef M y `endif"), ["begin", "x", ";", "end"]);
    }
}

/// A file `include can read: anything but a directory, so device files
/// such as `/dev/null` work too.
fn includable(p: &Path) -> bool {
    p.exists() && !p.is_dir()
}

/// Checks `1ns / 1ps`: magnitudes 1, 10 or 100, precision no coarser than
/// the unit.
fn timescale_error(spec: &str) -> Option<String> {
    const UNITS: &[&str] = &["fs", "ps", "ns", "us", "ms", "s"];
    let value = |s: &str| -> Option<i32> {
        let digits = s.find(|c: char| !c.is_ascii_digit())?;
        let mag = match &s[..digits] {
            "1" => 0,
            "10" => 1,
            "100" => 2,
            _ => return None,
        };
        let unit = UNITS.iter().position(|u| *u == &s[digits..])? as i32;
        Some(unit * 3 + mag)
    };
    let Some((unit, prec)) = spec.split_once('/') else {
        return Some("`timescale needs unit / precision".into());
    };
    match (value(unit), value(prec)) {
        (Some(u), Some(p)) if p > u => Some("`timescale precision is coarser than its unit".into()),
        (Some(_), Some(_)) => None,
        _ => Some(format!("invalid `timescale {spec}")),
    }
}

fn is_directive_name(s: &str) -> bool {
    matches!(
        s,
        "define"
            | "undef"
            | "undefineall"
            | "ifdef"
            | "ifndef"
            | "elsif"
            | "else"
            | "endif"
            | "include"
            | "begin_keywords"
            | "end_keywords"
            | "__FILE__"
            | "__LINE__"
    ) || LINE_DIRECTIVES.contains(&s)
}
