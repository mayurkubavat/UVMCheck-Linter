//! System subroutine and system timing check calls (IEEE 1800 chapter 20,
//! 21 and 31): each known name takes a fixed range of arguments.
//!
//! The ranges are the ones slang enforces. An argument left empty
//! (`$past(a,,,@(posedge clk))`) still counts. Names the table does not
//! know (PLI tasks, vendor extensions) are not checked; only an unknown
//! timing check in a `specify` block is an error.

use sv_syntax::{Child, NodeId, SyntaxKind, Tree};
use SyntaxKind::*;

use crate::names::Diag;

/// (name, fewest, most) arguments, sorted by name.
const SYSTEM_CALLS: &[(&str, u32, u32)] = &[
    ("$acos", 1, 1),
    ("$acosh", 1, 1),
    ("$asin", 1, 1),
    ("$asinh", 1, 1),
    ("$assertcontrol", 1, u32::MAX),
    ("$async$and$array", 3, 3),
    ("$async$and$plane", 3, 3),
    ("$async$nand$array", 3, 3),
    ("$async$nand$plane", 3, 3),
    ("$async$nor$array", 3, 3),
    ("$async$nor$plane", 3, 3),
    ("$async$or$array", 3, 3),
    ("$async$or$plane", 3, 3),
    ("$atan", 1, 1),
    ("$atan2", 2, 2),
    ("$atanh", 1, 1),
    ("$bits", 1, 1),
    ("$bitstoreal", 1, 1),
    ("$bitstoshortreal", 1, 1),
    ("$cast", 2, 2),
    ("$ceil", 1, 1),
    ("$changed", 1, 2),
    ("$changed_gclk", 1, 1),
    ("$changing_gclk", 1, 1),
    ("$clog2", 1, 1),
    ("$cos", 1, 1),
    ("$cosh", 1, 1),
    ("$countbits", 2, u32::MAX),
    ("$countdrivers", 1, 6),
    ("$countones", 1, 1),
    ("$coverage_control", 4, 4),
    ("$coverage_get", 3, 3),
    ("$coverage_get_max", 3, 3),
    ("$coverage_merge", 2, u32::MAX),
    ("$coverage_save", 2, u32::MAX),
    ("$deposit", 2, 2),
    ("$dimensions", 1, 1),
    ("$dist_chi_square", 2, 2),
    ("$dist_erlang", 3, 3),
    ("$dist_exponential", 2, 2),
    ("$dist_normal", 3, 3),
    ("$dist_poisson", 2, 2),
    ("$dist_t", 2, 2),
    ("$dist_uniform", 3, 3),
    ("$dumpall", 0, 0),
    ("$dumpflush", 0, 0),
    ("$dumplimit", 1, 1),
    ("$dumpoff", 0, 0),
    ("$dumpon", 0, 0),
    ("$dumpportslimit", 1, u32::MAX),
    ("$exit", 0, 0),
    ("$exp", 1, 1),
    ("$falling_gclk", 1, 1),
    ("$fclose", 1, 1),
    ("$fdisplay", 1, u32::MAX),
    ("$fdisplayb", 1, u32::MAX),
    ("$fdisplayh", 1, u32::MAX),
    ("$fdisplayo", 1, u32::MAX),
    ("$fell", 1, 2),
    ("$fell_gclk", 1, 1),
    ("$feof", 1, 1),
    ("$ferror", 2, 2),
    ("$fflush", 0, 1),
    ("$fgetc", 1, 1),
    ("$fgets", 2, 2),
    ("$finish", 0, 1),
    ("$floor", 1, 1),
    ("$fmonitor", 1, u32::MAX),
    ("$fmonitorb", 1, u32::MAX),
    ("$fmonitorh", 1, u32::MAX),
    ("$fmonitoro", 1, u32::MAX),
    ("$fopen", 1, 2),
    ("$fread", 2, 4),
    ("$fscanf", 2, u32::MAX),
    ("$fseek", 3, 3),
    ("$fstrobe", 1, u32::MAX),
    ("$fstrobeb", 1, u32::MAX),
    ("$fstrobeh", 1, u32::MAX),
    ("$fstrobeo", 1, u32::MAX),
    ("$ftell", 1, 1),
    ("$future_gclk", 1, 1),
    ("$fwrite", 1, u32::MAX),
    ("$fwriteb", 1, u32::MAX),
    ("$fwriteh", 1, u32::MAX),
    ("$fwriteo", 1, u32::MAX),
    ("$get_coverage", 0, 0),
    ("$getpattern", 1, 1),
    ("$global_clock", 0, 0),
    ("$high", 1, 2),
    ("$hypot", 2, 2),
    ("$increment", 1, 2),
    ("$incsave", 1, 1),
    ("$inferred_clock", 0, 0),
    ("$inferred_disable", 0, 0),
    ("$input", 1, 1),
    ("$isunbounded", 1, 1),
    ("$isunknown", 1, 1),
    ("$itor", 1, 1),
    ("$left", 1, 2),
    ("$list", 0, 1),
    ("$ln", 1, 1),
    ("$load_coverage_db", 1, 1),
    ("$log10", 1, 1),
    ("$low", 1, 2),
    ("$monitoroff", 0, 0),
    ("$monitoron", 0, 0),
    ("$nokey", 0, 0),
    ("$nolog", 0, 0),
    ("$onehot", 1, 1),
    ("$onehot0", 1, 1),
    ("$past", 1, 4),
    ("$past_gclk", 1, 1),
    ("$pow", 2, 2),
    ("$psprintf", 1, u32::MAX),
    ("$q_add", 4, 4),
    ("$q_exam", 4, 4),
    ("$q_full", 2, 2),
    ("$q_initialize", 4, 4),
    ("$q_remove", 4, 4),
    ("$readmemb", 2, 4),
    ("$readmemh", 2, 4),
    ("$realtime", 0, 0),
    ("$realtobits", 1, 1),
    ("$reset", 0, 3),
    ("$reset_count", 0, 0),
    ("$reset_value", 0, 0),
    ("$restart", 1, 1),
    ("$rewind", 1, 1),
    ("$right", 1, 2),
    ("$rising_gclk", 1, 1),
    ("$rose", 1, 2),
    ("$rose_gclk", 1, 1),
    ("$rtoi", 1, 1),
    ("$sampled", 1, 1),
    ("$save", 1, 1),
    ("$scale", 1, 1),
    ("$scope", 1, 1),
    ("$sdf_annotate", 1, u32::MAX),
    ("$set_coverage_db_name", 1, 1),
    ("$sformat", 2, u32::MAX),
    ("$sformatf", 1, u32::MAX),
    ("$shortrealtobits", 1, 1),
    ("$showscopes", 0, 1),
    ("$signed", 1, 1),
    ("$sin", 1, 1),
    ("$sinh", 1, 1),
    ("$size", 1, 2),
    ("$sqrt", 1, 1),
    ("$sreadmemb", 4, u32::MAX),
    ("$sreadmemh", 4, u32::MAX),
    ("$sscanf", 2, u32::MAX),
    ("$stable", 1, 2),
    ("$stable_gclk", 1, 1),
    ("$stacktrace", 0, 0),
    ("$steady_gclk", 1, 1),
    ("$stime", 0, 0),
    ("$stop", 0, 1),
    ("$swrite", 1, u32::MAX),
    ("$swriteb", 1, u32::MAX),
    ("$swriteh", 1, u32::MAX),
    ("$swriteo", 1, u32::MAX),
    ("$sync$and$array", 3, 3),
    ("$sync$and$plane", 3, 3),
    ("$sync$nand$array", 3, 3),
    ("$sync$nand$plane", 3, 3),
    ("$sync$nor$array", 3, 3),
    ("$sync$nor$plane", 3, 3),
    ("$sync$or$array", 3, 3),
    ("$sync$or$plane", 3, 3),
    ("$tan", 1, 1),
    ("$tanh", 1, 1),
    ("$test$plusargs", 1, 1),
    ("$time", 0, 0),
    ("$typename", 1, 1),
    ("$ungetc", 2, 2),
    ("$unpacked_dimensions", 1, 1),
    ("$unsigned", 1, 1),
    ("$urandom", 0, 1),
    ("$urandom_range", 1, 2),
    ("$value$plusargs", 2, 2),
    ("$writememb", 2, 4),
    ("$writememh", 2, 4),
];

/// System timing checks (31.2): (name, fewest, most).
const TIMING_CHECKS: &[(&str, u32, u32)] = &[
    ("$fullskew", 4, 7),
    ("$hold", 3, 4),
    ("$nochange", 4, 5),
    ("$period", 2, 3),
    ("$recovery", 3, 4),
    ("$recrem", 4, 9),
    ("$removal", 3, 4),
    ("$setup", 3, 4),
    ("$setuphold", 4, 9),
    ("$skew", 3, 4),
    ("$timeskew", 3, 6),
    ("$width", 2, 4),
];

fn arity(table: &[(&str, u32, u32)], name: &str) -> Option<(u32, u32)> {
    table.binary_search_by(|e| e.0.cmp(name)).ok().map(|i| (table[i].1, table[i].2))
}

/// Arguments between the parentheses of `n`: commas plus one, or none
/// for `()`.
fn count(t: &Tree, n: NodeId) -> u32 {
    let mut commas = 0;
    let mut open = false;
    let mut any = false;
    for c in &t.node(n).children {
        match c {
            Child::Token(i) => match &*t.tokens[*i as usize].text {
                "(" => open = true,
                "," => commas += 1,
                _ => {}
            },
            Child::Node(_) => any |= open,
        }
    }
    if commas == 0 && !any {
        0
    } else {
        commas + 1
    }
}

fn arg_error(name: &str, (lo, hi): (u32, u32), n: u32) -> Option<String> {
    if n < lo {
        Some(format!("too few arguments for '{name}'; expected {lo} but {n} were provided"))
    } else if n > hi {
        Some(format!("too many arguments for '{name}'; expected {hi} but {n} were provided"))
    } else {
        None
    }
}

pub fn check(trees: &[Tree]) -> Vec<Diag> {
    let mut out = Vec::new();
    for (tree, t) in trees.iter().enumerate() {
        for n in t.preorder() {
            let message = match t.kind(n) {
                CallExpr => {
                    let Some(callee) = t.children(n).next() else { continue };
                    let Some(args) = t.child(n, ArgList) else { continue };
                    if t.kind(callee) != NameExpr {
                        continue;
                    }
                    let name = t.bare_text(callee);
                    let Some(r) = arity(SYSTEM_CALLS, &name) else { continue };
                    arg_error(&name, r, count(t, args))
                }
                // `$name;` as a statement: no arguments.
                ExprStmt => {
                    let Some(e) = t.children(n).next() else { continue };
                    if t.kind(e) != NameExpr {
                        continue;
                    }
                    let name = t.bare_text(e);
                    let Some(r) = arity(SYSTEM_CALLS, &name) else { continue };
                    arg_error(&name, r, 0)
                }
                TimingCheck => {
                    let Some(Child::Token(i)) = t.node(n).children.first() else { continue };
                    let name = &*t.tokens[*i as usize].text;
                    match arity(TIMING_CHECKS, name) {
                        Some(r) => arg_error(name, r, count(t, n)),
                        None => Some(format!("unknown system timing check '{name}'")),
                    }
                }
                _ => None,
            };
            if let Some(message) = message {
                out.push(Diag { tree, node: n, message });
            }
        }
    }
    out
}
