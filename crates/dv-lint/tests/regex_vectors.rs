//! dv-lint's patterns against std::regex's recorded answers
//! (testdata/regex/vectors.tsv, written by tools/gates/regex_gate.py), so the
//! parity holds without a C++ build.

fn unescape(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] != b'\\' || i + 1 >= b.len() {
            out.push(b[i]);
            i += 1;
            continue;
        }
        i += 1;
        match b[i] {
            b't' => out.push(b'\t'),
            b'n' => out.push(b'\n'),
            b'r' => out.push(b'\r'),
            b'x' if i + 2 < b.len() => {
                out.push(u8::from_str_radix(&s[i + 1..i + 3], 16).expect("hex escape"));
                i += 2;
            }
            n => out.push(n),
        }
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

#[test]
fn patterns_match_std_regex() {
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/../../testdata/regex/vectors.tsv");
    let text = std::fs::read_to_string(path).expect("testdata/regex/vectors.tsv");
    let mut bad = Vec::new();
    let mut n = 0;
    for line in text.lines().filter(|l| !l.starts_with('#')) {
        let f: Vec<&str> = line.splitn(4, '\t').collect();
        let [pattern, flags, input, want] = f[..] else { panic!("bad vector line: {line:?}") };
        n += 1;
        let got = match dv_lint::pattern::Pattern::new(&unescape(pattern), flags == "i") {
            Err(_) => "err".to_string(),
            Ok(p) if want == "err" => format!("loads ({p:?})"),
            Ok(p) => {
                let t = unescape(input);
                format!("{} {}", u8::from(p.is_match_anywhere(&t)), u8::from(p.is_full_match(&t)))
            }
        };
        if got != want {
            bad.push(format!("{pattern:?} {flags:?} {input:?}: std::regex {want}, dv-lint {got}"));
        }
    }
    assert!(n > 1000, "only {n} vectors");
    assert!(bad.is_empty(), "{} of {n} vectors differ:\n{}", bad.len(), bad[..bad.len().min(20)].join("\n"));
}
