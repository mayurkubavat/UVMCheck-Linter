//! SV-EVO-002 (ChangeCoupling): With --history: reports two files in different
//! packages that change together in at least three commits and in at least half
//! of the commits touching either.

use crate::support::history::{base_name, file_of, log_for, max_commits, Log, HISTORY};
use crate::syntax::Cst;
use crate::{Severity, Sink};
use std::collections::{BTreeMap, BTreeSet};
use std::sync::atomic::Ordering;
use std::sync::Arc;
use svlang::syntax::SyntaxKind::*;

/// SV-EVO-002: two files in different packages that keep changing together.
pub(crate) fn change_coupling(_: &dv_model::Design, csts: &[Cst], out: &mut Sink) {
    if !HISTORY.load(Ordering::Relaxed) {
        return;
    }
    let (min_shared, min_degree, max_files) = (
        crate::options::get("SV-EVO-002", "min_shared_commits", 3) as usize,
        crate::options::get("SV-EVO-002", "min_degree", 50) as usize,
        crate::options::get("SV-EVO-002", "max_commit_files", 30) as usize,
    );
    struct Info {
        packages: BTreeSet<String>,
        /// The file's first class in a package: its tree and name token.
        first: (usize, u32),
    }
    let mut files: BTreeMap<String, Info> = BTreeMap::new();
    for (k, c) in csts.iter().enumerate() {
        let classes = crate::support::class_index::Classes::new(c);
        for i in classes.user() {
            let cls = &classes.list[i];
            let Some(p) = c.parent(cls.node).filter(|&p| c.kind(p) == PackageDecl) else { continue };
            let pkg = crate::support::slang_ast::decl_name(c, p);
            let file = file_of(c, cls.tok);
            if file.is_empty() {
                continue;
            }
            files.entry(file).or_insert(Info { packages: BTreeSet::new(), first: (k, cls.tok) }).packages.insert(pkg);
        }
    }
    // Commits per file, leaving out sweeping commits (reformatting, renames,
    // licence headers) that touch many files at once.
    let mut revs: Vec<(&String, Arc<Log>, Vec<u32>)> = Vec::new();
    for file in files.keys() {
        let Some(log) = log_for(file, max_commits("SV-EVO-002")) else { continue };
        let Some(cs) = log.files.get(file) else { continue };
        let kept: Vec<u32> = cs.iter().copied().filter(|&c| log.sizes[c as usize] <= max_files).collect();
        if kept.len() >= min_shared {
            revs.push((file, log.clone(), kept));
        }
    }
    for i in 0..revs.len() {
        for j in i + 1..revs.len() {
            let (fa, la, ra) = &revs[i];
            let (fb, lb, rb) = &revs[j];
            if !Arc::ptr_eq(la, lb) {
                continue;
            }
            let (pa, pb) = (&files[*fa].packages, &files[*fb].packages);
            if pa.intersection(pb).next().is_some() {
                continue;
            }
            // Both lists are ascending (commit indices, newest first).
            let shared = ra.iter().filter(|c| rb.binary_search(c).is_ok()).count();
            let total = ra.len() + rb.len();
            if shared < min_shared || shared * 200 < min_degree * total {
                continue;
            }
            let (k, tok) = files[*fb].first;
            out.add(
                "SV-EVO-002",
                "ChangeCoupling",
                Severity::Info,
                format!(
                    "'{}' (package {}) changed together with '{}' (package {}) in {} commits, {}% of the commits \
                     touching either. Packages that keep changing together share knowledge their imports do not \
                     show; move it to one place or make the dependency explicit.",
                    base_name(fb),
                    pb.iter().next().map_or("", String::as_str),
                    base_name(fa),
                    pa.iter().next().map_or("", String::as_str),
                    shared,
                    shared * 200 / total
                ),
                csts[k].loc(tok),
            );
        }
    }
}
