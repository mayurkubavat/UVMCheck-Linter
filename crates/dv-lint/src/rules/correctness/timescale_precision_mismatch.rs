//! SV-TIM-001 (TimescalePrecisionMismatch): Flags `timescale directives where
//! the time precision is coarser than the time unit (IEEE 1800 § 3.14.2).

use crate::support::data_items::main_files;
use crate::syntax::Cst;
use crate::{Severity, Sink};
use svlang::syntax::Origin;

fn time_exp(s: &str) -> i32 {
    let mut s = s.trim_start_matches([' ', '\t']).trim_end_matches([' ', '\t', '\r', '\n', ';']);
    let mult = if let Some(r) = s.strip_prefix("100") {
        s = r;
        2
    } else if let Some(r) = s.strip_prefix("10") {
        s = r;
        1
    } else if let Some(r) = s.strip_prefix('1') {
        s = r;
        0
    } else {
        return 999;
    };
    let base = match s.trim_start_matches([' ', '\t']) {
        "s" => 0,
        "ms" => -3,
        "us" => -6,
        "ns" => -9,
        "ps" => -12,
        "fs" => -15,
        _ => return 999,
    };
    base + mult
}

pub(crate) fn timescale_precision(c: &Cst, s: &mut Sink) {
    for f in main_files(c) {
        let src = &c.tree.sources[f as usize];
        let b = src.as_bytes();
        let mut pos = 0;
        while let Some(i) = src[pos..].find("`timescale") {
            let at = pos + i;
            if at == 0 || matches!(b[at - 1], b'\n' | b' ' | b'\t') {
                let end = src[at..].find(['\r', '\n']).map_or(src.len(), |e| at + e);
                let line = &src[at + 10..end.max(at + 10)];
                if let Some(slash) = line.find('/') {
                    let (unit, prec) = (&line[..slash], &line[slash + 1..]);
                    let (ue, pe) = (time_exp(unit), time_exp(prec));
                    if ue != 999 && pe != 999 && pe > ue {
                        let cu = unit.trim_matches([' ', '\t']);
                        let cp = prec.trim_end_matches([' ', '\t', '\r', '\n', ';']).trim_start_matches([' ', '\t']);
                        s.add(
                            "SV-TIM-001",
                            "TimescalePrecisionMismatch",
                            Severity::Error,
                            format!(
                                "Timescale directive specifies a time precision ('{cp}') coarser than the time unit ('{cu}'). Per IEEE 1800 § 3.14.2, time precision must be smaller than or equal to the time unit to prevent simulation rounding errors."
                            ),
                            c.loc_at(Origin { file: f, offset: at as u32 }),
                        );
                    }
                }
            }
            pos = at + 10;
        }
    }
}
