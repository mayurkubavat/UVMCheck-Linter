//! The UVM API packs (data/uvm-api/*.json, made by
//! tools/codegen/gen_uvm_api_pack.py): for each UVM release, which classes,
//! members and macros the library tags Accellera-only, and which macros it
//! defines only as deprecated API. Rules UVM-API-001 and UVM-API-002 read
//! the pack of the release being compiled.

use std::collections::HashSet;
use std::sync::OnceLock;

const EMBEDDED: &[&str] = &[
    include_str!("../../../data/uvm-api/2020-2.0.json"),
    include_str!("../../../data/uvm-api/2020.3.0.json"),
    include_str!("../../../data/uvm-api/2020.3.1.json"),
];

/// What one pack says.
pub(crate) struct ApiPack {
    pub version: String,
    /// Classes tagged `accellera`.
    pub accellera_classes: HashSet<String>,
    /// (class, member) tagged `accellera`.
    pub accellera_members: HashSet<(String, String)>,
    /// Macros defined only under UVM_ENABLE_DEPRECATED_API.
    pub deprecated_macros: HashSet<String>,
}

// ---- a small JSON reader: the packs are the only JSON dv-lint reads ----

enum Json {
    Null,
    Bool(bool),
    Num,
    Str(String),
    Arr,
    Obj(Vec<(String, Json)>),
}

impl Json {
    fn get(&self, k: &str) -> Option<&Json> {
        match self {
            Json::Obj(v) => v.iter().find(|(n, _)| n == k).map(|(_, x)| x),
            _ => None,
        }
    }
    fn str(&self) -> Option<&str> {
        match self {
            Json::Str(s) => Some(s),
            _ => None,
        }
    }
    fn entries(&self) -> &[(String, Json)] {
        match self {
            Json::Obj(v) => v,
            _ => &[],
        }
    }
}

struct Reader<'a> {
    s: &'a [u8],
    i: usize,
}

impl Reader<'_> {
    fn ws(&mut self) {
        while self.i < self.s.len() && self.s[self.i].is_ascii_whitespace() {
            self.i += 1;
        }
    }

    fn value(&mut self) -> Option<Json> {
        self.ws();
        match *self.s.get(self.i)? {
            b'{' => {
                self.i += 1;
                let mut out = Vec::new();
                loop {
                    self.ws();
                    if self.s.get(self.i) == Some(&b'}') {
                        self.i += 1;
                        return Some(Json::Obj(out));
                    }
                    let Json::Str(k) = self.value()? else { return None };
                    self.ws();
                    if self.s.get(self.i) != Some(&b':') {
                        return None;
                    }
                    self.i += 1;
                    let v = self.value()?;
                    out.push((k, v));
                    self.ws();
                    if self.s.get(self.i) == Some(&b',') {
                        self.i += 1;
                    }
                }
            }
            b'[' => {
                self.i += 1;
                loop {
                    self.ws();
                    if self.s.get(self.i) == Some(&b']') {
                        self.i += 1;
                        return Some(Json::Arr);
                    }
                    self.value()?;
                    self.ws();
                    if self.s.get(self.i) == Some(&b',') {
                        self.i += 1;
                    }
                }
            }
            b'"' => {
                self.i += 1;
                let mut out = String::new();
                loop {
                    let c = *self.s.get(self.i)?;
                    self.i += 1;
                    match c {
                        b'"' => return Some(Json::Str(out)),
                        b'\\' => {
                            let e = *self.s.get(self.i)?;
                            self.i += 1;
                            match e {
                                b'n' => out.push('\n'),
                                b't' => out.push('\t'),
                                b'r' => out.push('\r'),
                                b'b' | b'f' => {}
                                b'u' => {
                                    let hex = std::str::from_utf8(self.s.get(self.i..self.i + 4)?).ok()?;
                                    self.i += 4;
                                    out.push(char::from_u32(u32::from_str_radix(hex, 16).ok()?).unwrap_or('?'));
                                }
                                other => out.push(other as char),
                            }
                        }
                        _ => {
                            // Copy a whole UTF-8 sequence.
                            let start = self.i - 1;
                            let mut end = self.i;
                            while end < self.s.len() && (self.s[end] & 0xC0) == 0x80 {
                                end += 1;
                            }
                            out.push_str(std::str::from_utf8(&self.s[start..end]).ok()?);
                            self.i = end;
                        }
                    }
                }
            }
            b't' => {
                self.i += 4;
                Some(Json::Bool(true))
            }
            b'f' => {
                self.i += 5;
                Some(Json::Bool(false))
            }
            b'n' => {
                self.i += 4;
                Some(Json::Null)
            }
            _ => {
                while self.i < self.s.len() && matches!(self.s[self.i], b'-' | b'+' | b'.' | b'e' | b'E' | b'0'..=b'9') {
                    self.i += 1;
                }
                Some(Json::Num)
            }
        }
    }
}

fn parse(text: &str) -> Option<ApiPack> {
    let root = Reader { s: text.as_bytes(), i: 0 }.value()?;
    let version = root.get("version")?.str()?.to_string();
    let mut p = ApiPack {
        version,
        accellera_classes: HashSet::new(),
        accellera_members: HashSet::new(),
        deprecated_macros: HashSet::new(),
    };
    let tag = |j: &Json| j.get("tag").and_then(Json::str) == Some("accellera");
    for (name, info) in root.get("classes").map_or(&[][..], Json::entries) {
        if tag(info) {
            p.accellera_classes.insert(name.clone());
        }
        for (m, mi) in info.get("members").map_or(&[][..], Json::entries) {
            if tag(mi) {
                p.accellera_members.insert((name.clone(), m.clone()));
            }
        }
    }
    for (name, info) in root.get("macros").map_or(&[][..], Json::entries) {
        if matches!(info.get("deprecated"), Some(Json::Bool(true))) {
            p.deprecated_macros.insert(name.clone());
        }
    }
    Some(p)
}

/// "2020-2.0" < "2020.3.0": the numbers in order.
fn version_key(v: &str) -> Vec<u32> {
    v.split(|c: char| !c.is_ascii_digit()).filter(|s| !s.is_empty()).filter_map(|s| s.parse().ok()).collect()
}

fn packs() -> &'static [ApiPack] {
    static PACKS: OnceLock<Vec<ApiPack>> = OnceLock::new();
    PACKS.get_or_init(|| {
        let mut v: Vec<ApiPack> = match std::env::var("UVMCHECK_UVM_API_PACK") {
            Ok(path) if !path.is_empty() => std::fs::read_to_string(&path).ok().and_then(|t| parse(&t)).into_iter().collect(),
            _ => EMBEDDED.iter().filter_map(|t| parse(t)).collect(),
        };
        v.sort_by_key(|p| version_key(&p.version));
        v
    })
}

/// "Accellera:1800.2:UVM:2020.3.1" -> "2020.3.1"; "Accellera:1800.2-2020:UVM:2.0"
/// -> "2020-2.0"; "UVM-1.2" -> "1.2".
fn normalize(s: &str) -> String {
    let parts: Vec<&str> = s.split(':').collect();
    let last = parts.last().copied().unwrap_or("");
    let last = last.strip_prefix("UVM-").unwrap_or(last);
    let mut v = last.to_string();
    if !v.starts_with("20") && !v.starts_with("1.") && parts.len() > 1 {
        if let Some(d) = parts[1].rfind('-') {
            v = format!("{}-{v}", &parts[1][d + 1..]);
        }
    }
    v
}

fn line_of(v: &str) -> String {
    if v.starts_with("2020") {
        "2020".into()
    } else if v.starts_with("2017") {
        "2017".into()
    } else if v.starts_with("1.") {
        "1".into()
    } else {
        v.split(['.', '-']).next().unwrap_or("").to_string()
    }
}

/// The pack for a UVM version string (`uvm_pkg::UVM_VERSION_STRING` or
/// `uvm_revision`); the newest when unknown.
pub(crate) fn for_version(version: Option<&str>) -> Option<&'static ApiPack> {
    let all = packs();
    let Some(v) = version.map(normalize) else { return all.last() };
    if let Some(p) = all.iter().find(|p| p.version == v) {
        return Some(p);
    }
    all.iter().rev().find(|p| line_of(&p.version) == line_of(&v)).or(all.last())
}

