//! The result cache (`.uvmcheck_cache/`), with uvmcheck's semantics: a run is
//! replayed only when its environment key matches and every file the last
//! run read still has the same bytes; anything else, a corrupt entry
//! included, is a miss and a full run. Hashes are of file bytes (uvmcheck
//! deliberately keeps no signature-based hashing either).
//!
//! The environment key hashes the tool version, this binary's build (size
//! and modification time), the whole command line, and the bytes of the
//! files that steer the run without being sources: the project config and
//! its layers, `-f`/`-F` file lists, the waiver file and baseline (given or
//! the default `.uvmcheck/` ones), and Python rule sources.
//!
//! What is cached is the rules' output before the project config, waivers
//! and baseline are applied (those run again on a replay, cheaply), so a hit
//! gives the same report as a full run. uvmcheck also keeps per-unit entries
//! and merges them on a partial hit, but it still compiles and runs every
//! rule then, so only a complete hit saves time; this cache keeps the one
//! entry that can be replayed. Its files live under `native/` in the cache
//! folder, apart from the C++ engine's.

use std::path::{Path, PathBuf};

use crate::fix::{Fix, FixSafety};
use crate::json::{obj, parse, Json};
use crate::{Finding, Severity};
use dv_model::Loc;

const FORMAT: u32 = 1;

/// uvmcheck's two-lane FNV-1a hash, 32 hex digits.
#[derive(Clone)]
pub struct Hasher {
    a: u64,
    b: u64,
}

impl Default for Hasher {
    fn default() -> Self {
        Hasher { a: 14_695_981_039_346_656_037, b: 0x6C62_272E_07BB_0142 }
    }
}

impl Hasher {
    const PRIME: u64 = 1_099_511_628_211;

    pub fn add(&mut self, s: &[u8]) {
        self.a ^= s.len() as u64;
        self.a = self.a.wrapping_mul(Self::PRIME);
        self.b ^= (s.len() as u64).wrapping_mul(0x9E37_79B1_85EB_CA87);
        for &c in s {
            self.a ^= u64::from(c);
            self.a = self.a.wrapping_mul(Self::PRIME);
            self.b ^= u64::from(c);
            self.b = self.b.wrapping_mul(Self::PRIME);
        }
    }

    pub fn hex(&self) -> String {
        format!("{:016x}{:016x}", self.a, self.b)
    }
}

fn hash_file(p: &Path) -> Option<String> {
    let bytes = std::fs::read(p).ok()?;
    let mut h = Hasher::default();
    h.add(&bytes);
    Some(h.hex())
}

/// The running binary's build: size and modification time, so a rebuilt
/// linter never replays another build's findings.
fn build_identity() -> String {
    let id = std::env::current_exe().ok().and_then(|e| std::fs::metadata(e).ok()).and_then(|m| {
        let t = m.modified().ok()?.duration_since(std::time::UNIX_EPOCH).ok()?;
        Some(format!("{}-{}", m.len(), t.as_nanos()))
    });
    id.unwrap_or_else(|| "unknown".into())
}

/// The cache of one run.
pub struct Cache {
    dir: PathBuf,
    key: String,
}

/// A replayed run: the file table, the rules' findings, and how many
/// findings inline waiver comments had dropped.
pub struct Hit {
    pub files: Vec<String>,
    pub found: Vec<Finding>,
    pub inline_waived: usize,
    pub units: usize,
}

impl Cache {
    /// The cache in `dir` for a run with these `args` and steering `inputs`
    /// (files whose bytes are part of the key; a folder stands for the
    /// `.py` files under it; a missing file counts as missing).
    pub fn new(dir: &Path, args: &[String], inputs: &[PathBuf]) -> Cache {
        let mut h = Hasher::default();
        for part in [env!("CARGO_PKG_VERSION"), &build_identity(), &FORMAT.to_string()] {
            h.add(part.as_bytes());
        }
        for a in args {
            h.add(a.as_bytes());
        }
        h.add(b"|");
        let mut files = Vec::new();
        for f in inputs {
            if f.is_dir() {
                py_files(f, &mut files);
            } else {
                files.push(f.clone());
            }
        }
        files.sort();
        files.dedup();
        for f in files {
            h.add(f.to_string_lossy().as_bytes());
            match std::fs::read(&f) {
                Ok(b) => h.add(&b),
                Err(_) => h.add(b"missing"),
            }
        }
        Cache { dir: dir.join("native"), key: h.hex() }
    }

    fn manifest(&self) -> PathBuf {
        self.dir.join("manifest.json")
    }

    fn blob(&self, key: &str) -> PathBuf {
        self.dir.join("m").join(format!("{key}.json"))
    }

    /// The cached run, when the key matches and no recorded file changed.
    pub fn load(&self) -> Option<Hit> {
        self.lookup().ok()
    }

    /// [`Cache::load`], or on a miss the recorded files that changed (empty
    /// when the entry is missing, corrupt, or for other options).
    pub fn lookup(&self) -> Result<Hit, Vec<String>> {
        let none = || Vec::new();
        let m = std::fs::read_to_string(self.manifest()).ok().and_then(|t| parse(&t).ok()).ok_or_else(none)?;
        if m.get("format").as_u32() != Some(FORMAT) || m.get("env").as_str() != Some(self.key.as_str()) {
            return Err(none());
        }
        let mut changed = Vec::new();
        for d in m.get("inputs").as_arr() {
            let (p, h) = (d.get("path").as_str().unwrap_or(""), d.get("hash").as_str().unwrap_or(""));
            if hash_file(Path::new(p)).as_deref() != Some(h) {
                changed.push(p.rsplit('/').next().unwrap_or(p).to_string());
            }
        }
        if !changed.is_empty() {
            return Err(changed);
        }
        self.replay(&m).ok_or_else(none)
    }

    fn replay(&self, m: &Json) -> Option<Hit> {
        let blob = parse(&std::fs::read_to_string(self.blob(m.get("merged").as_str()?)).ok()?).ok()?;
        if blob.get("format").as_u32() != Some(FORMAT) {
            return None;
        }
        let files: Vec<String> =
            blob.get("files").as_arr().iter().map(|f| f.as_str().map(str::to_string)).collect::<Option<_>>()?;
        let found = blob.get("findings").as_arr().iter().map(read_finding).collect::<Option<Vec<_>>>()?;
        Some(Hit {
            files,
            found,
            inline_waived: blob.get("inlineWaived").as_u32()? as usize,
            units: m.get("units").as_u32().unwrap_or(0) as usize,
        })
    }

    /// Stores a run that read `deps` (sources, includes, libraries) and
    /// compiled `units` source trees. Failures leave no usable entry and are
    /// otherwise ignored.
    pub fn store(
        &self,
        deps: &[PathBuf],
        units: usize,
        files: &[String],
        found: &[Finding],
        inline_waived: usize,
    ) -> bool {
        let mut inputs = Vec::new();
        let mut paths: Vec<PathBuf> = deps.iter().map(|d| d.canonicalize().unwrap_or_else(|_| d.clone())).collect();
        paths.sort();
        paths.dedup();
        let mut merged = Hasher::default();
        merged.add(self.key.as_bytes());
        for p in &paths {
            let Some(h) = hash_file(p) else { return false };
            merged.add(p.to_string_lossy().as_bytes());
            merged.add(h.as_bytes());
            inputs.push(obj(vec![("path", p.to_string_lossy().into_owned().into()), ("hash", h.into())]));
        }
        let merged = merged.hex();
        let blob = obj(vec![
            ("format", FORMAT.into()),
            ("inlineWaived", (inline_waived as u32).into()),
            ("files", Json::Arr(files.iter().map(|f| f.as_str().into()).collect())),
            ("findings", Json::Arr(found.iter().map(write_finding).collect())),
        ]);
        let manifest = obj(vec![
            ("format", FORMAT.into()),
            ("tool", env!("CARGO_PKG_VERSION").into()),
            ("env", self.key.as_str().into()),
            ("merged", merged.as_str().into()),
            ("units", (units as u32).into()),
            ("inputs", Json::Arr(inputs)),
        ]);
        write_atomic(&self.blob(&merged), &blob.to_string()) && write_atomic(&self.manifest(), &manifest.to_string())
    }
}

/// Deletes the cache folder (`--clear-cache`); failures are ignored.
pub fn clear(dir: &Path) {
    let _ = std::fs::remove_dir_all(dir);
}

fn py_files(d: &Path, out: &mut Vec<PathBuf>) {
    let Ok(rd) = std::fs::read_dir(d) else { return };
    for e in rd.flatten() {
        let p = e.path();
        if p.is_dir() {
            py_files(&p, out);
        } else if p.extension().is_some_and(|x| x == "py") {
            out.push(p);
        }
    }
}

fn write_atomic(path: &Path, body: &str) -> bool {
    if let Some(d) = path.parent() {
        if std::fs::create_dir_all(d).is_err() {
            return false;
        }
    }
    // A name of this process's own: runs sharing the folder never write
    // into one temporary file.
    static N: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    let n = N.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let tmp = path.with_extension(format!("json.{}.{n}.tmp", std::process::id()));
    std::fs::write(&tmp, body).is_ok() && std::fs::rename(&tmp, path).is_ok()
}

fn write_loc(l: Loc) -> Json {
    let mut v = vec![("file", l.file.into()), ("line", l.line.into()), ("column", l.column.into())];
    if let Some((f, line)) = l.expansion {
        v.push(("expansion", Json::Arr(vec![f.into(), line.into()])));
    }
    obj(v)
}

fn read_loc(j: &Json) -> Option<Loc> {
    let e = j.get("expansion").as_arr();
    let expansion = match e {
        [f, l] => Some((f.as_u32()?, l.as_u32()?)),
        _ => None,
    };
    Some(Loc {
        file: j.get("file").as_u32()?,
        line: j.get("line").as_u32()?,
        column: j.get("column").as_u32()?,
        expansion,
    })
}

fn write_finding(f: &Finding) -> Json {
    let related = f.related.iter().map(|(l, m)| obj(vec![("at", write_loc(*l)), ("message", m.as_str().into())]));
    let fixes = f.fixes.iter().map(|x| {
        obj(vec![
            ("path", x.path.as_str().into()),
            ("start", x.start.into()),
            ("end", x.end.into()),
            ("text", x.text.as_str().into()),
            ("description", x.description.as_str().into()),
            ("safety", x.safety.as_str().into()),
        ])
    });
    obj(vec![
        ("rule", f.rule.into()),
        ("name", f.name.into()),
        ("severity", f.severity.as_str().into()),
        ("message", f.message.as_str().into()),
        ("at", write_loc(f.loc)),
        ("related", Json::Arr(related.collect())),
        ("fixes", Json::Arr(fixes.collect())),
    ])
}

/// A rule ID or name as the `&'static str` findings hold: the registered one
/// when there is one (Python rules' are leaked, a few per run).
fn intern(s: &str) -> &'static str {
    let known = crate::rules().into_iter().flat_map(|r| {
        let subs = crate::sub_rules(r.id).iter().flat_map(|s| [s.id, s.name, s.finding_name]);
        [r.id, r.name].into_iter().chain(subs)
    });
    for k in known {
        if k == s {
            return k;
        }
    }
    Box::leak(s.to_string().into_boxed_str())
}

pub(crate) fn severity_of(s: &str) -> Option<Severity> {
    Some(match s {
        "INFO" => Severity::Info,
        "STYLE" => Severity::Style,
        "WARNING" => Severity::Warning,
        "ERROR" => Severity::Error,
        _ => return None,
    })
}

fn read_finding(j: &Json) -> Option<Finding> {
    let related = j
        .get("related")
        .as_arr()
        .iter()
        .map(|r| Some((read_loc(r.get("at"))?, r.get("message").as_str()?.to_string())))
        .collect::<Option<Vec<_>>>()?;
    let fixes = j
        .get("fixes")
        .as_arr()
        .iter()
        .map(|x| {
            Some(Fix {
                path: x.get("path").as_str()?.to_string(),
                start: x.get("start").as_u32()?,
                end: x.get("end").as_u32()?,
                text: x.get("text").as_str()?.to_string(),
                description: x.get("description").as_str()?.to_string(),
                safety: match x.get("safety").as_str()? {
                    "safe" => FixSafety::Safe,
                    "unsafe" => FixSafety::Unsafe,
                    _ => FixSafety::DisplayOnly,
                },
            })
        })
        .collect::<Option<Vec<_>>>()?;
    Some(Finding {
        rule: intern(j.get("rule").as_str()?),
        name: intern(j.get("name").as_str()?),
        severity: severity_of(j.get("severity").as_str()?)?,
        message: j.get("message").as_str()?.to_string(),
        loc: read_loc(j.get("at"))?,
        related,
        fixes,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("dv_lint_cache_{}_{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn replays_until_a_dependency_changes() {
        let d = tmp("replay");
        let src = d.join("a.sv");
        std::fs::write(&src, "module a; endmodule\n").unwrap();
        let args = vec!["a.sv".to_string()];
        let c = Cache::new(&d.join("cache"), &args, &[]);
        assert!(c.load().is_none());
        let f = Finding {
            rule: "SV-COR-008",
            name: "x",
            severity: Severity::Warning,
            message: "m \"q\"".into(),
            loc: Loc { file: 1, line: 2, column: 3, expansion: Some((1, 4)) },
            related: vec![(Loc { file: 1, line: 1, column: 1, expansion: None }, "n".into())],
            fixes: vec![],
        };
        let files = vec![String::new(), src.display().to_string()];
        assert!(c.store(std::slice::from_ref(&src), 1, &files, std::slice::from_ref(&f), 2));
        let hit = Cache::new(&d.join("cache"), &args, &[]).load().expect("a hit");
        assert_eq!(hit.files, files);
        assert_eq!(hit.inline_waived, 2);
        assert_eq!(hit.found[0].message, f.message);
        assert_eq!(hit.found[0].loc.expansion, Some((1, 4)));
        // Other arguments: another key.
        assert!(Cache::new(&d.join("cache"), &["b.sv".to_string()], &[]).load().is_none());
        std::fs::write(&src, "module a; wire w; endmodule\n").unwrap();
        assert!(Cache::new(&d.join("cache"), &args, &[]).load().is_none());
        let _ = std::fs::remove_dir_all(&d);
    }
}
