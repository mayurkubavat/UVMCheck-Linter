//! SV-EVO-001 (Hotspot): With --history: reports the files that are both
//! complex (summed cyclomatic complexity of their routines) and often changed
//! in git, the top five by complexity times commits.

use crate::support::history::{base_name, file_of, log_for, max_commits, HISTORY};
use crate::syntax::Cst;
use crate::{Severity, Sink};
use std::collections::BTreeMap;
use std::sync::atomic::Ordering;

/// SV-EVO-001: the five files with the largest complexity times churn.
pub(crate) fn hotspot(_: &dv_model::Design, csts: &[Cst], out: &mut Sink) {
    if !HISTORY.load(Ordering::Relaxed) {
        return;
    }
    let (top_files, min_commits, min_complexity) = (
        crate::options::get("SV-EVO-001", "top_files", 5) as usize,
        crate::options::get("SV-EVO-001", "min_commits", 3) as usize,
        crate::options::get("SV-EVO-001", "min_complexity", 15),
    );
    struct Stats {
        complexity: i64,
        worst: i64,
        /// The most complex routine: its tree and name token, and name.
        at: Option<(usize, u32, String)>,
        churn: usize,
        log_size: usize,
    }
    let mut files: BTreeMap<String, Stats> = BTreeMap::new();
    for (k, c) in csts.iter().enumerate() {
        let classes = crate::support::class_index::Classes::new(c);
        for r in crate::support::routine_metrics::user_routines(c, &classes) {
            let file = file_of(c, r.tok);
            if file.is_empty() {
                continue;
            }
            let cc = crate::support::routine_metrics::routine_metrics(c, r.body).cyclomatic;
            let f = files.entry(file).or_insert(Stats { complexity: 0, worst: 0, at: None, churn: 0, log_size: 0 });
            // Each module instance lists its routines again.
            f.complexity += cc * r.times as i64;
            if f.at.is_none() || cc > f.worst {
                f.worst = cc;
                f.at = Some((k, r.tok, r.name.clone()));
            }
        }
    }
    let mut ranked: Vec<(i64, String)> = Vec::new();
    for (file, f) in files.iter_mut() {
        let Some(log) = log_for(file, max_commits("SV-EVO-001")) else { continue };
        f.churn = log.files.get(file).map_or(0, Vec::len);
        f.log_size = log.sizes.len();
        if f.churn < min_commits || f.complexity < min_complexity {
            continue;
        }
        ranked.push((f.churn as i64 * f.complexity, file.clone()));
    }
    ranked.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| a.1.cmp(&b.1)));
    for (i, (_, file)) in ranked.iter().take(top_files).enumerate() {
        let f = &files[file];
        let Some((k, tok, name)) = &f.at else { continue };
        out.add(
            "SV-EVO-001",
            "Hotspot",
            Severity::Info,
            format!(
                "Hotspot #{}: '{}' changed in {} of the last {} commits and its routines add up to cyclomatic \
                 complexity {}; '{}' ({}) is the most complex. Complex code that keeps changing is where defects \
                 collect; simplify or split it before adding to it.",
                i + 1,
                base_name(file),
                f.churn,
                f.log_size,
                f.complexity,
                name,
                f.worst
            ),
            csts[*k].loc(*tok),
        );
    }
}
