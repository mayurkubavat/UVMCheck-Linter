//! Message, header and naming patterns, as uvmcheck reads them: ECMAScript
//! regular expressions in `std::regex`'s default grammar, matched on bytes in
//! the C locale.
//!
//! Matching is the `regex` crate's, on bytes with Unicode off, so `\w`,
//! `\d`, `\s`, `\b` and case folding are ASCII as in `std::regex`, and the
//! time is linear in the input. The crate's grammar is not ECMAScript, so a
//! pattern is first checked here: what ECMAScript reads one way and the
//! crate another (a leading `]` in a class, a nested `[`, `(?i)` flags,
//! `\A`, `\x{..}`) is refused, as are the ECMAScript features the crate
//! lacks (backreferences, lookahead). So a pattern either means what it
//! means to uvmcheck or does not load: it never quietly matches something
//! else, and a waiver never hides findings its author did not name.
//! `tools/gates/regex_gate.py` checks this against `std::regex`.
//!
//! One rewrite: ECMAScript's `.` matches neither `\n` nor `\r`, the crate's
//! only excludes `\n`, so `.` outside a class becomes `[^\n\r]`.

use regex::bytes::{Regex, RegexBuilder};

/// A compiled pattern.
#[derive(Clone, Debug)]
pub struct Pattern {
    search: Regex,
    full: Regex,
}

impl Pattern {
    /// Compiles `pattern`; `icase` folds ASCII case (`std::regex::icase`).
    /// The error says what is wrong, in ECMAScript's terms where it can.
    pub fn new(pattern: &str, icase: bool) -> Result<Pattern, String> {
        let translated = translate(pattern)?;
        let build = |p: &str| {
            RegexBuilder::new(p)
                .unicode(false)
                .case_insensitive(icase)
                .size_limit(1 << 20)
                .build()
                .map_err(|e| crate_error(&e.to_string()))
        };
        Ok(Pattern { search: build(&translated)?, full: build(&format!(r"\A(?:{translated})\z"))? })
    }

    /// The pattern matches somewhere in `text` (`std::regex_search`).
    pub fn is_match_anywhere(&self, text: &str) -> bool {
        self.search.is_match(text.as_bytes())
    }

    /// The pattern matches all of `text` (`std::regex_match`).
    pub fn is_full_match(&self, text: &str) -> bool {
        self.full.is_match(text.as_bytes())
    }
}

/// The crate's message, minus its caret diagram: the last line names the
/// problem.
fn crate_error(e: &str) -> String {
    e.lines().rev().find(|l| l.trim_start().starts_with("error:")).map_or(e, str::trim).to_string()
}

/// Checks `p` as ECMAScript and rewrites it for the crate.
fn translate(p: &str) -> Result<String, String> {
    let b = p.as_bytes();
    let mut out = String::with_capacity(p.len() + 8);
    let mut i = 0;
    // What a quantifier here would repeat.
    let mut last = Last::Atom;
    while i < b.len() {
        let c = b[i];
        match c {
            b'\\' => {
                let Some(&n) = b.get(i + 1) else { return Err("trailing backslash".into()) };
                escape(n, b.get(i + 2).copied(), false)?;
                out.push('\\');
                out.push(n as char);
                i += 2;
                last = if n == b'b' { Last::Assertion } else { Last::Atom };
            }
            b'[' => {
                let end = class(b, i, &mut out)?;
                i = end;
                last = Last::Atom;
            }
            b'(' => {
                if b.get(i + 1) == Some(&b'?') {
                    match b.get(i + 2) {
                        Some(b':') => {}
                        Some(b'=') | Some(b'!') => return Err("lookahead (?= ) and (?! ) are not supported".into()),
                        Some(b'<') => return Err("lookbehind and named groups are not ECMAScript".into()),
                        _ => return Err("inline flags and other (? groups are not ECMAScript".into()),
                    }
                    out.push_str("(?:");
                    i += 3;
                } else {
                    out.push('(');
                    i += 1;
                }
                last = Last::Atom;
            }
            b'{' => {
                // `x{,n}` is the crate's, not ECMAScript's.
                if b.get(i + 1) == Some(&b',') {
                    return Err("'{,n}' is not an ECMAScript quantifier".into());
                }
                let close = b[i..].iter().position(|&x| x == b'}').ok_or("an unterminated '{'")?;
                let body = &p[i + 1..i + close];
                let ok = !body.is_empty()
                    && body.split(',').count() <= 2
                    && body.split(',').next().is_some_and(|m| !m.is_empty())
                    && body.bytes().all(|x| x.is_ascii_digit() || x == b',');
                if !ok {
                    return Err(format!("'{{{body}}}' is not a quantifier; write '\\{{' for a literal brace"));
                }
                last.quantify(false)?;
                out.push_str(&p[i..=i + close]);
                i += close + 1;
                last = Last::Quantified;
            }
            b'*' | b'+' | b'?' => {
                let lazy = c == b'?' && matches!(last, Last::Quantified);
                last.quantify(lazy)?;
                out.push(c as char);
                i += 1;
                last = if lazy { Last::Lazy } else { Last::Quantified };
            }
            b'^' | b'$' => {
                out.push(c as char);
                i += 1;
                last = Last::Assertion;
            }
            // std::regex refuses a lone ']' or '}'; the crate reads a literal.
            b']' | b'}' => return Err(format!("an unescaped '{}'; write '\\{}'", c as char, c as char)),
            b'.' => {
                out.push_str(r"[^\n\r]");
                i += 1;
                last = Last::Atom;
            }
            _ if c >= 0x80 => {
                let ch = p[i..].chars().next().unwrap_or('\u{fffd}');
                out.push(ch);
                i += ch.len_utf8();
                last = Last::NonAscii;
            }
            _ => {
                // `#` and whitespace are literal in both (no `x` flag).
                out.push(c as char);
                i += 1;
                last = Last::Atom;
            }
        }
    }
    Ok(out)
}

/// What the item before a quantifier was.
#[derive(Clone, Copy)]
enum Last {
    Atom,
    /// A quantified atom: only a lazy '?' may follow.
    Quantified,
    /// A lazily quantified atom: nothing may follow.
    Lazy,
    /// `^ $ \b`: std::regex refuses a quantifier on one.
    Assertion,
    /// A non-ASCII character, several bytes: std::regex repeats only the
    /// last byte, the crate the whole character.
    NonAscii,
}

impl Last {
    /// A quantifier may follow (`lazy`: the '?' that makes one lazy).
    fn quantify(self, lazy: bool) -> Result<(), String> {
        match self {
            Last::Atom => Ok(()),
            Last::Quantified if lazy => Ok(()),
            Last::Quantified | Last::Lazy => Err("a quantifier on a quantifier".into()),
            Last::Assertion => Err("a quantifier on an assertion (^, $ or \\b)".into()),
            Last::NonAscii => Err("a quantifier after a non-ASCII character".into()),
        }
    }
}

/// An escape `\n` (with `next` the character after it): accepted only where
/// ECMAScript and the crate agree, or where the crate rejects it too.
fn escape(n: u8, next: Option<u8>, in_class: bool) -> Result<(), String> {
    match n {
        // Classes, and the control escapes both read alike.
        b'd' | b'D' | b'w' | b'W' | b's' | b'S' | b'f' | b'n' | b'r' | b't' | b'v' => Ok(()),
        b'b' if in_class => Err(r"'\b' in a class (a backspace in ECMAScript) is not supported".into()),
        b'b' => Ok(()),
        // std::regex implementations disagree on '\B' (libc++ finds one
        // inside a one-letter word), so it has no single meaning to keep.
        b'B' => Err(r"'\B' is not supported (std::regex implementations disagree on it)".into()),
        b'x' if next == Some(b'{') => Err(r"'\x{..}' is not ECMAScript; write '\xHH'".into()),
        b'x' => Ok(()),
        b'u' => Err(r"'\u' escapes are not supported; write the character or '\xHH'".into()),
        b'c' => Err(r"'\c' control escapes are not supported".into()),
        b'0'..=b'9' => Err("backreferences and octal escapes are not supported".into()),
        // Any other letter: an error in ECMAScript or a different meaning in
        // the crate (\A, \z, \p, \Q ...).
        _ if n.is_ascii_alphabetic() => Err(format!("'\\{}' is not an ECMAScript escape", n as char)),
        _ if n >= 0x80 => Err("an escaped non-ASCII character".into()),
        // Escaped punctuation is the character itself in both.
        _ => Ok(()),
    }
}

/// A class `[...]` starting at `start`; writes it to `out` and returns the
/// index after its `]`.
fn class(b: &[u8], start: usize, out: &mut String) -> Result<usize, String> {
    let mut i = start + 1;
    out.push('[');
    if b.get(i) == Some(&b'^') {
        out.push('^');
        i += 1;
    }
    // ECMAScript: `[]` matches nothing and `[^]` anything; the crate reads
    // `]` there as a literal.
    if b.get(i) == Some(&b']') {
        return Err("'[]' and '[^]' are not supported; escape a literal ']' as '\\]'".into());
    }
    while i < b.len() {
        let c = b[i];
        match c {
            b']' => {
                out.push(']');
                return Ok(i + 1);
            }
            b'\\' => {
                let Some(&n) = b.get(i + 1) else { return Err("trailing backslash".into()) };
                escape(n, b.get(i + 2).copied(), true)?;
                out.push('\\');
                out.push(n as char);
                i += 2;
            }
            b'[' => match b.get(i + 1) {
                // POSIX classes and their kin, read alike.
                Some(&k @ (b':' | b'.' | b'=')) => {
                    let close = [k, b']'];
                    let end = b[i + 2..].windows(2).position(|w| w == close).ok_or("unterminated '[:' in a class")?;
                    if k != b':' {
                        return Err("collating elements '[. .]' and '[= =]' are not supported".into());
                    }
                    let body = &b[i..i + 2 + end + 2];
                    out.push_str(std::str::from_utf8(body).map_err(|_| "a non-ASCII class name")?);
                    i += body.len();
                }
                // A literal '[' in ECMAScript, a nested class in the crate.
                _ => return Err("an unescaped '[' inside a class; write '\\['".into()),
            },
            // The crate's set operators; literal in ECMAScript.
            b'&' | b'-' | b'~' if b.get(i + 1) == Some(&c) => {
                return Err(format!("'{0}{0}' inside a class; escape one of them", c as char));
            }
            _ if c >= 0x80 => return Err("a non-ASCII character inside a class".into()),
            _ => {
                out.push(c as char);
                i += 1;
            }
        }
    }
    Err("unterminated character class".into())
}

#[cfg(test)]
mod tests {
    use super::Pattern;

    fn search(p: &str, t: &str) -> bool {
        Pattern::new(p, false).unwrap().is_match_anywhere(t)
    }

    #[test]
    fn ecmascript_basics() {
        assert!(search("width.*8", "literal width 8"));
        assert!(!search("width.*8", "literal width 16"));
        assert!(search(r"^UVM_\w+$", "UVM_LOW"));
        assert!(search(r"\bcb\b", "drive cb now"));
        assert!(!search(r"\bcb\b", "drive vcb now"));
        assert!(search("a|b", "xbx"));
        assert!(search("(?:ab)+c", "ababc"));
        assert!(search("[[:digit:]]{2}", "x42"));
        assert!(search(r"\.", "a.b"));
        assert!(!search(r"\.", "ab"));
    }

    #[test]
    fn dot_skips_carriage_returns() {
        assert!(!search("a.b", "a\rb"));
        assert!(!search("a.b", "a\nb"));
        assert!(search("a[^x]b", "a\rb"));
    }

    #[test]
    fn ascii_only_classes_and_case() {
        assert!(!search(r"\w", "é"));
        assert!(Pattern::new("uvm_info", true).unwrap().is_match_anywhere("`UVM_INFO"));
        assert!(!Pattern::new("é", true).unwrap().is_match_anywhere("É"));
    }

    #[test]
    fn full_match() {
        let p = Pattern::new("[a-z_]+_t", false).unwrap();
        assert!(p.is_full_match("my_type_t"));
        assert!(!p.is_full_match("my_type_t2"));
        let p = Pattern::new("a|ab", false).unwrap();
        assert!(p.is_full_match("ab"));
    }

    #[test]
    fn refuses_what_would_mean_something_else() {
        for p in [
            r"(a)\1", "(?=a)", "(?!a)", "(?i)a", "(?<n>a)", r"\Aa", r"a\z", r"\pL", r"\x{41}", r"\u0041", "[]a]",
            "[^]", "[[a]", "[a&&b]", "[a--b]", r"[\b]", "a{,3}", "é+", "[é]", "(", "[a", "\\",
        ] {
            assert!(Pattern::new(p, false).is_err(), "{p} should not load");
        }
    }
}
