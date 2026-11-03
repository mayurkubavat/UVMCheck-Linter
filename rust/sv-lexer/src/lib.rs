//! Lossless SystemVerilog lexer (IEEE 1800-2023), the first piece of the
//! native front end (plan stage R2).
//!
//! Every byte of the input lands in exactly one token, whitespace and
//! comments included, so the concatenated token text is the input again.
//! Preprocessor directives and macro uses are tokens here (`Directive`,
//! `MacroQuote`, ...); expanding them is the preprocessor's job.
//!
//! ```
//! let src = "module m; // top\nendmodule\n";
//! let toks = sv_lexer::lex(src);
//! assert_eq!(toks.iter().map(|t| t.text(src)).collect::<String>(), src);
//! ```

mod keywords;
pub use keywords::{is_keyword, KEYWORDS};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Kind {
    // Trivia
    Whitespace,
    Newline,
    LineComment,
    BlockComment,
    /// A backslash ending a line, as in a multi-line `define body.
    LineContinuation,
    // Names
    Identifier,
    /// `\name` up to the next whitespace.
    EscapedIdentifier,
    Keyword,
    /// `$display`, `$urandom`.
    SystemIdentifier,
    // Preprocessor
    /// A backtick and a name: `define, `include, `uvm_info.
    Directive,
    /// `"
    MacroQuote,
    /// `\`"
    MacroEscapedQuote,
    /// ``
    MacroPaste,
    // Literals
    /// Decimal digits; also the size in front of a based literal.
    IntegerLiteral,
    /// `'h`, `'sb`: the base of a based literal (the size, if any, is the
    /// token before; the digits are the next non-trivia token).
    IntegerBase,
    /// The digits after an `IntegerBase`: `1F`, `0101`, `x0_z?`.
    VectorDigits,
    /// `'0`, `'1`, `'x`, `'z`.
    UnbasedUnsizedLiteral,
    RealLiteral,
    /// `10ns`, `1.5ps`.
    TimeLiteral,
    StringLiteral,
    /// `"""..."""` (1800-2023).
    TripleQuotedString,
    /// Operators and punctuation, longest match.
    Punct,
    /// Anything else: a stray byte, an unterminated comment or string.
    Unknown,
}

impl Kind {
    pub fn is_trivia(self) -> bool {
        matches!(
            self,
            Kind::Whitespace | Kind::Newline | Kind::LineComment | Kind::BlockComment | Kind::LineContinuation
        )
    }
}

/// A token: a kind and a byte range of the source.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Token {
    pub kind: Kind,
    pub start: u32,
    pub len: u32,
}

impl Token {
    pub fn text<'a>(&self, src: &'a str) -> &'a str {
        &src[self.start as usize..(self.start + self.len) as usize]
    }
}

/// Lexes `src` completely; never fails. Inputs over 4 GiB are not supported.
pub fn lex(src: &str) -> Vec<Token> {
    let mut lx = Lexer { b: src.as_bytes(), pos: 0, out: Vec::with_capacity(src.len() / 4), pending_digits: false };
    while lx.pos < lx.b.len() {
        let start = lx.pos;
        let kind = lx.next_kind();
        debug_assert!(lx.pos > start);
        lx.out.push(Token { kind, start: start as u32, len: (lx.pos - start) as u32 });
    }
    lx.out
}

// Longest first within each length class.
const PUNCT4: &[&[u8]] = &[b"<<<=", b">>>="];
const PUNCT3: &[&[u8]] = &[
    b"<<<", b">>>", b"===", b"!==", b"==?", b"!=?", b"<<=", b">>=", b"->>", b"|->", b"|=>", b"#-#", b"#=#",
    b"&&&", b"<->", b"+/-", b"+%-",
];
const PUNCT2: &[&[u8]] = &[
    b"==", b"!=", b"<=", b">=", b"&&", b"||", b"**", b"<<", b">>", b"->", b"++", b"--", b"+=", b"-=", b"*=",
    b"/=", b"%=", b"&=", b"|=", b"^=", b"~&", b"~|", b"~^", b"^~", b"::", b":=", b":/", b"##", b"@@", b"+:",
    b"-:", b"'{", b"*>", b"=>",
];
const PUNCT1: &[u8] = b"()[]{};:,.?#@=+-*/%&|^~!<>$'";

struct Lexer<'a> {
    b: &'a [u8],
    pos: usize,
    out: Vec<Token>,
    /// An `IntegerBase` was just lexed: its digits come next.
    pending_digits: bool,
}

fn is_ident_start(c: u8) -> bool {
    c.is_ascii_alphabetic() || c == b'_'
}
fn is_ident_char(c: u8) -> bool {
    c.is_ascii_alphanumeric() || c == b'_' || c == b'$'
}
fn is_based_digit(c: u8) -> bool {
    c.is_ascii_hexdigit() || matches!(c, b'x' | b'X' | b'z' | b'Z' | b'?' | b'_')
}

impl<'a> Lexer<'a> {
    fn peek(&self, k: usize) -> u8 {
        *self.b.get(self.pos + k).unwrap_or(&0)
    }
    fn starts_with(&self, s: &[u8]) -> bool {
        self.b[self.pos..].starts_with(s)
    }
    fn eat_while(&mut self, f: impl Fn(u8) -> bool) {
        while self.pos < self.b.len() && f(self.b[self.pos]) {
            self.pos += 1;
        }
    }
    /// One whole UTF-8 character, so token ranges stay on char boundaries.
    fn eat_char(&mut self) {
        self.pos += 1;
        while self.pos < self.b.len() && (self.b[self.pos] & 0xC0) == 0x80 {
            self.pos += 1;
        }
    }

    fn next_kind(&mut self) -> Kind {
        let c = self.peek(0);
        if self.pending_digits && is_based_digit(c) {
            self.pending_digits = false;
            self.eat_while(is_based_digit);
            return Kind::VectorDigits;
        }
        if !matches!(c, b' ' | b'\t') {
            self.pending_digits = false;
        }
        match c {
            b' ' | b'\t' | 0x0B | 0x0C => {
                self.eat_while(|c| matches!(c, b' ' | b'\t' | 0x0B | 0x0C));
                Kind::Whitespace
            }
            b'\n' => {
                self.pos += 1;
                Kind::Newline
            }
            b'\r' => {
                self.pos += if self.peek(1) == b'\n' { 2 } else { 1 };
                Kind::Newline
            }
            b'/' if self.peek(1) == b'/' => {
                self.eat_while(|c| c != b'\n' && c != b'\r');
                Kind::LineComment
            }
            b'/' if self.peek(1) == b'*' => {
                match self.b[self.pos + 2..].windows(2).position(|w| w == b"*/") {
                    Some(i) => {
                        self.pos += 2 + i + 2;
                        Kind::BlockComment
                    }
                    None => {
                        self.pos = self.b.len();
                        Kind::Unknown
                    }
                }
            }
            b'\\' => {
                if self.peek(1) == b'\n' || self.peek(1) == b'\r' {
                    self.pos += 1;
                    Kind::LineContinuation
                } else if self.peek(1) == 0 || self.peek(1).is_ascii_whitespace() {
                    self.pos += 1;
                    Kind::Unknown
                } else {
                    self.pos += 1;
                    while self.pos < self.b.len() && !self.b[self.pos].is_ascii_whitespace() {
                        self.eat_char();
                    }
                    Kind::EscapedIdentifier
                }
            }
            b'`' => self.backtick(),
            b'$' if is_ident_char(self.peek(1)) => {
                self.pos += 1;
                self.eat_while(is_ident_char);
                Kind::SystemIdentifier
            }
            b'"' => self.string(),
            b'\'' => self.apostrophe(),
            b'0'..=b'9' => self.number(),
            _ if is_ident_start(c) => {
                let start = self.pos;
                self.eat_while(is_ident_char);
                let word = std::str::from_utf8(&self.b[start..self.pos]).unwrap_or("");
                if is_keyword(word) {
                    Kind::Keyword
                } else {
                    Kind::Identifier
                }
            }
            _ => self.punct(),
        }
    }

    fn backtick(&mut self) -> Kind {
        if self.starts_with(b"`\\`\"") {
            self.pos += 4;
            Kind::MacroEscapedQuote
        } else if self.starts_with(b"`\"") {
            self.pos += 2;
            Kind::MacroQuote
        } else if self.starts_with(b"``") {
            self.pos += 2;
            Kind::MacroPaste
        } else if is_ident_start(self.peek(1)) {
            self.pos += 1;
            self.eat_while(is_ident_char);
            Kind::Directive
        } else {
            self.pos += 1;
            Kind::Unknown
        }
    }

    fn string(&mut self) -> Kind {
        if self.starts_with(b"\"\"\"") {
            self.pos += 3;
            while self.pos < self.b.len() {
                if self.starts_with(b"\\") {
                    self.pos += 1;
                    if self.pos < self.b.len() {
                        self.eat_char();
                    }
                } else if self.starts_with(b"\"\"\"") {
                    self.pos += 3;
                    return Kind::TripleQuotedString;
                } else {
                    self.eat_char();
                }
            }
            return Kind::Unknown;
        }
        self.pos += 1;
        while self.pos < self.b.len() {
            match self.b[self.pos] {
                b'"' => {
                    self.pos += 1;
                    return Kind::StringLiteral;
                }
                b'\\' => {
                    // An escape; a backslash-newline continues the string.
                    self.pos += 1;
                    if self.starts_with(b"\r\n") {
                        self.pos += 2;
                    } else if self.pos < self.b.len() {
                        self.eat_char();
                    }
                }
                b'\n' | b'\r' => return Kind::Unknown, // unterminated
                _ => self.eat_char(),
            }
        }
        Kind::Unknown
    }

    fn apostrophe(&mut self) -> Kind {
        let c1 = self.peek(1);
        if matches!(c1, b'0' | b'1' | b'x' | b'X' | b'z' | b'Z') && !is_ident_char(self.peek(2)) {
            self.pos += 2;
            return Kind::UnbasedUnsizedLiteral;
        }
        let mut k = 1;
        if matches!(self.peek(k), b's' | b'S') {
            k += 1;
        }
        if matches!(self.peek(k), b'b' | b'B' | b'o' | b'O' | b'd' | b'D' | b'h' | b'H') {
            // Digits may follow after whitespace: 8'h FF.
            let mut j = k + 1;
            while matches!(self.peek(j), b' ' | b'\t') {
                j += 1;
            }
            if is_based_digit(self.peek(j)) {
                self.pos += k + 1;
                self.pending_digits = true;
                return Kind::IntegerBase;
            }
        }
        self.punct()
    }

    fn number(&mut self) -> Kind {
        self.eat_while(|c| c.is_ascii_digit() || c == b'_');
        let mut kind = Kind::IntegerLiteral;
        if self.peek(0) == b'.' && self.peek(1).is_ascii_digit() {
            self.pos += 1;
            self.eat_while(|c| c.is_ascii_digit() || c == b'_');
            kind = Kind::RealLiteral;
        }
        if matches!(self.peek(0), b'e' | b'E')
            && (self.peek(1).is_ascii_digit()
                || (matches!(self.peek(1), b'+' | b'-') && self.peek(2).is_ascii_digit()))
        {
            self.pos += 2;
            self.eat_while(|c| c.is_ascii_digit() || c == b'_');
            kind = Kind::RealLiteral;
        }
        for unit in [&b"ms"[..], b"us", b"ns", b"ps", b"fs", b"s"] {
            if self.starts_with(unit) && !is_ident_char(self.peek(unit.len())) {
                self.pos += unit.len();
                return Kind::TimeLiteral;
            }
        }
        kind
    }

    fn punct(&mut self) -> Kind {
        for p in PUNCT4 {
            if self.starts_with(p) {
                self.pos += 4;
                return Kind::Punct;
            }
        }
        for p in PUNCT3 {
            if self.starts_with(p) {
                self.pos += 3;
                return Kind::Punct;
            }
        }
        for p in PUNCT2 {
            if self.starts_with(p) {
                self.pos += 2;
                return Kind::Punct;
            }
        }
        if PUNCT1.contains(&self.peek(0)) {
            self.pos += 1;
            return Kind::Punct;
        }
        self.eat_char();
        Kind::Unknown
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kinds(src: &str) -> Vec<(Kind, &str)> {
        lex(src).into_iter().filter(|t| !t.kind.is_trivia()).map(|t| (t.kind, t.text(src))).collect()
    }

    #[test]
    fn literals() {
        use Kind::*;
        assert_eq!(
            kinds("8'hFF 'sb01 'd 9 '0 1.5e3 10ns 2.5ps 42 1_000"),
            [
                (IntegerLiteral, "8"),
                (IntegerBase, "'h"),
                (VectorDigits, "FF"),
                (IntegerBase, "'sb"),
                (VectorDigits, "01"),
                (IntegerBase, "'d"),
                (VectorDigits, "9"),
                (UnbasedUnsizedLiteral, "'0"),
                (RealLiteral, "1.5e3"),
                (TimeLiteral, "10ns"),
                (TimeLiteral, "2.5ps"),
                (IntegerLiteral, "42"),
                (IntegerLiteral, "1_000"),
            ]
        );
    }

    #[test]
    fn names_strings_and_macros() {
        use Kind::*;
        assert_eq!(
            kinds(r#"`uvm_info("ID", $sformatf("%0d", x), UVM_LOW) \bus[0] `"a`" `\`" a``b"#),
            [
                (Directive, "`uvm_info"),
                (Punct, "("),
                (StringLiteral, "\"ID\""),
                (Punct, ","),
                (SystemIdentifier, "$sformatf"),
                (Punct, "("),
                (StringLiteral, "\"%0d\""),
                (Punct, ","),
                (Identifier, "x"),
                (Punct, ")"),
                (Punct, ","),
                (Identifier, "UVM_LOW"),
                (Punct, ")"),
                (EscapedIdentifier, "\\bus[0]"),
                (MacroQuote, "`\""),
                (Identifier, "a"),
                (MacroQuote, "`\""),
                (MacroEscapedQuote, "`\\`\""),
                (Identifier, "a"),
                (MacroPaste, "``"),
                (Identifier, "b"),
            ]
        );
    }

    #[test]
    fn operators_longest_match() {
        let k: Vec<_> = kinds("a <<<= b; c |-> d; e[->1]; f '{1}; g ==? h").into_iter().map(|(_, t)| t).collect();
        assert_eq!(k, ["a", "<<<=", "b", ";", "c", "|->", "d", ";", "e", "[", "->", "1", "]", ";", "f", "'{", "1", "}", ";", "g", "==?", "h"]);
    }

    #[test]
    fn keywords_and_triple_strings() {
        use Kind::*;
        assert_eq!(
            kinds("class c extends uvm_object; string s = \"\"\"a\n\"b\"\"\"; endclass"),
            [
                (Keyword, "class"),
                (Identifier, "c"),
                (Keyword, "extends"),
                (Identifier, "uvm_object"),
                (Punct, ";"),
                (Keyword, "string"),
                (Identifier, "s"),
                (Punct, "="),
                (TripleQuotedString, "\"\"\"a\n\"b\"\"\""),
                (Punct, ";"),
                (Keyword, "endclass"),
            ]
        );
    }

    #[test]
    fn lossless_even_on_garbage() {
        for src in ["", "/* open", "\"open\n", "`", "\\", "é ü 漢", "a\r\nb\\\n", "'", "8'h", "$"] {
            let toks = lex(src);
            assert_eq!(toks.iter().map(|t| t.text(src)).collect::<String>(), src, "{src:?}");
        }
    }
}
