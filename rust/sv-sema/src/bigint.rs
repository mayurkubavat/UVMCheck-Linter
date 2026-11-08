//! A signed integer of any size, for constant values wider than 128 bits
//! (cva6's configuration struct is 17270 bits). Two's complement in 64-bit
//! words, least significant first; the top bit of the last word repeats
//! above it. Only what constant evaluation needs.

use std::cmp::Ordering;

/// A signed integer of any width; see the module docs for the layout.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Int {
    /// Never empty; no redundant top word (one equal to the sign extension
    /// of the word below it).
    w: Words,
}

/// Words of an [`Int`]: up to [`INLINE`] in place (most values), more on
/// the heap.
#[derive(Clone)]
enum Words {
    Inline(u8, [u64; INLINE]),
    Heap(Vec<u64>),
}

const INLINE: usize = 4;

impl Words {
    fn new() -> Words {
        Words::Inline(0, [0; INLINE])
    }

    fn push(&mut self, v: u64) {
        match self {
            Words::Inline(n, a) if (*n as usize) < INLINE => {
                a[*n as usize] = v;
                *n += 1;
            }
            Words::Inline(n, a) => {
                let mut h = a[..*n as usize].to_vec();
                h.push(v);
                *self = Words::Heap(h);
            }
            Words::Heap(h) => h.push(v),
        }
    }

    fn pop(&mut self) -> Option<u64> {
        match self {
            Words::Inline(0, _) => None,
            Words::Inline(n, a) => {
                *n -= 1;
                Some(a[*n as usize])
            }
            Words::Heap(h) => h.pop(),
        }
    }
}

impl std::ops::Deref for Words {
    type Target = [u64];
    fn deref(&self) -> &[u64] {
        match self {
            Words::Inline(n, a) => &a[..*n as usize],
            Words::Heap(h) => h,
        }
    }
}

impl std::ops::DerefMut for Words {
    fn deref_mut(&mut self) -> &mut [u64] {
        match self {
            Words::Inline(n, a) => &mut a[..*n as usize],
            Words::Heap(h) => h,
        }
    }
}

impl PartialEq for Words {
    fn eq(&self, o: &Words) -> bool {
        **self == **o
    }
}

impl Eq for Words {}

impl std::fmt::Debug for Words {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        (**self).fmt(f)
    }
}

impl FromIterator<u64> for Words {
    fn from_iter<I: IntoIterator<Item = u64>>(it: I) -> Words {
        let mut w = Words::new();
        for v in it {
            w.push(v);
        }
        w
    }
}

impl From<Vec<u64>> for Words {
    fn from(v: Vec<u64>) -> Words {
        if v.len() <= INLINE {
            v.into_iter().collect()
        } else {
            Words::Heap(v)
        }
    }
}

impl Default for Int {
    fn default() -> Self {
        Int::from(0)
    }
}

impl From<i128> for Int {
    fn from(v: i128) -> Int {
        Int { w: [v as u64, (v >> 64) as u64].into_iter().collect() }.trim()
    }
}

impl From<i64> for Int {
    fn from(v: i64) -> Int {
        Int::from(v as i128)
    }
}

impl From<i32> for Int {
    fn from(v: i32) -> Int {
        Int::from(v as i128)
    }
}

impl From<bool> for Int {
    fn from(v: bool) -> Int {
        Int::from(v as i128)
    }
}

impl Int {
    fn sign_word(&self) -> u64 {
        if (*self.w.last().unwrap() as i64) < 0 {
            u64::MAX
        } else {
            0
        }
    }

    fn word(&self, i: usize) -> u64 {
        self.w.get(i).copied().unwrap_or_else(|| self.sign_word())
    }

    fn trim(mut self) -> Int {
        if self.w.is_empty() {
            self.w.push(0);
        }
        while self.w.len() > 1 {
            let n = self.w.len();
            let top = self.w[n - 1];
            let below_neg = (self.w[n - 2] as i64) < 0;
            if (top == 0 && !below_neg) || (top == u64::MAX && below_neg) {
                self.w.pop();
            } else {
                break;
            }
        }
        self
    }

    /// True when the value is below zero.
    pub fn is_neg(&self) -> bool {
        self.sign_word() != 0
    }

    /// True when the value is zero.
    pub fn is_zero(&self) -> bool {
        self.w.len() == 1 && self.w[0] == 0
    }

    /// The value if it fits in an i128.
    pub fn to_i128(&self) -> Option<i128> {
        if self.w.len() > 2 {
            return None;
        }
        Some(((self.word(1) as u128) << 64 | self.word(0) as u128) as i128)
    }

    /// The low 128 bits, sign-extended from there: an index or count.
    pub fn low_i128(&self) -> i128 {
        self.to_i128().unwrap_or(if self.is_neg() { i128::MIN } else { i128::MAX })
    }

    /// Bits needed to hold the value without a sign bit (for >= 0).
    pub fn active_bits(&self) -> u32 {
        let n = self.w.len();
        for i in (0..n).rev() {
            if self.w[i] != 0 {
                return i as u32 * 64 + (64 - self.w[i].leading_zeros());
            }
        }
        0
    }

    /// Bit `i` of the two's complement value (sign-extended above the top word).
    pub fn bit(&self, i: u32) -> bool {
        (self.word((i / 64) as usize) >> (i % 64)) & 1 == 1
    }

    /// Bits `lo..lo+n`, zero-extended.
    pub fn bits(&self, lo: u32, n: u32) -> Int {
        self.shr(lo).trunc(n, false)
    }

    /// The low `width` bits, sign-extended from the top one when `signed`.
    pub fn trunc(&self, width: u32, signed: bool) -> Int {
        let width = width.max(1);
        let words = width.div_ceil(64) as usize;
        let mut w: Words = (0..words).map(|i| self.word(i)).collect();
        let top = width % 64;
        if top != 0 {
            w[words - 1] &= (1u64 << top) - 1;
        }
        let neg = signed && (w[(width as usize - 1) / 64] >> ((width - 1) % 64)) & 1 == 1;
        if neg {
            if top != 0 {
                w[words - 1] |= !((1u64 << top) - 1);
            }
            Int { w }.trim()
        } else {
            w.push(0);
            Int { w }.trim()
        }
    }

    fn zip(&self, o: &Int, f: impl Fn(u64, u64) -> u64) -> Int {
        let n = self.w.len().max(o.w.len()) + 1;
        Int { w: (0..n).map(|i| f(self.word(i), o.word(i))).collect() }.trim()
    }

    /// Bitwise and.
    pub fn and(&self, o: &Int) -> Int {
        self.zip(o, |a, b| a & b)
    }
    /// Bitwise or.
    pub fn or(&self, o: &Int) -> Int {
        self.zip(o, |a, b| a | b)
    }
    /// Bitwise exclusive or.
    pub fn xor(&self, o: &Int) -> Int {
        self.zip(o, |a, b| a ^ b)
    }
    /// Bitwise not (`-self - 1`).
    pub fn not(&self) -> Int {
        Int { w: self.w.iter().map(|a| !a).collect() }.trim()
    }

    /// `self + o`, exact.
    pub fn add(&self, o: &Int) -> Int {
        let n = self.w.len().max(o.w.len()) + 1;
        let mut out = Words::new();
        let mut carry = 0u64;
        for i in 0..n {
            let (s1, c1) = self.word(i).overflowing_add(o.word(i));
            let (s2, c2) = s1.overflowing_add(carry);
            out.push(s2);
            carry = (c1 || c2) as u64;
        }
        Int { w: out.into() }.trim()
    }

    /// `-self`, exact.
    pub fn neg(&self) -> Int {
        self.not().add(&Int::from(1))
    }

    /// `self - o`, exact.
    pub fn sub(&self, o: &Int) -> Int {
        self.add(&o.neg())
    }

    /// `self << n`, exact (no width limit).
    pub fn shl(&self, n: u32) -> Int {
        let (ws, bs) = ((n / 64) as usize, n % 64);
        let mut out: Words = std::iter::repeat_n(0u64, ws).collect();
        let len = self.w.len() + 1;
        for i in 0..len {
            let lo = self.word(i);
            let below = if i == 0 { 0 } else { self.word(i - 1) };
            out.push(if bs == 0 { lo } else { (lo << bs) | (below >> (64 - bs)) });
        }
        Int { w: out.into() }.trim()
    }

    /// Arithmetic shift right (the sign repeats).
    pub fn shr(&self, n: u32) -> Int {
        let (ws, bs) = ((n / 64) as usize, n % 64);
        let len = self.w.len();
        if ws >= len {
            return Int { w: std::iter::once(self.sign_word()).collect() };
        }
        let mut out = Words::new();
        for i in ws..len {
            let lo = self.word(i);
            let hi = self.word(i + 1);
            out.push(if bs == 0 { lo } else { (lo >> bs) | (hi << (64 - bs)) });
        }
        Int { w: out.into() }.trim()
    }

    fn magnitude(&self) -> Vec<u64> {
        let m = if self.is_neg() { self.neg() } else { self.clone() };
        let mut w = m.w.to_vec();
        while w.len() > 1 && *w.last().unwrap() == 0 {
            w.pop();
        }
        w
    }

    fn from_mag(mut m: Vec<u64>, neg: bool) -> Int {
        m.push(0);
        let v = Int { w: m.into() }.trim();
        if neg {
            v.neg()
        } else {
            v
        }
    }

    /// `self * o`, exact.
    pub fn mul(&self, o: &Int) -> Int {
        if let (Some(a), Some(b)) = (self.to_i128(), o.to_i128()) {
            if let Some(p) = a.checked_mul(b) {
                return Int::from(p);
            }
        }
        let (a, b) = (self.magnitude(), o.magnitude());
        let mut r = vec![0u64; a.len() + b.len()];
        for (i, &x) in a.iter().enumerate() {
            let mut carry = 0u128;
            for (j, &y) in b.iter().enumerate() {
                let t = r[i + j] as u128 + x as u128 * y as u128 + carry;
                r[i + j] = t as u64;
                carry = t >> 64;
            }
            let mut k = i + b.len();
            while carry > 0 {
                let t = r[k] as u128 + carry;
                r[k] = t as u64;
                carry = t >> 64;
                k += 1;
            }
        }
        Int::from_mag(r, self.is_neg() != o.is_neg())
    }

    /// Quotient and remainder, truncating toward zero (`/`, `%`); None
    /// when dividing by zero.
    pub fn divrem(&self, o: &Int) -> Option<(Int, Int)> {
        if o.is_zero() {
            return None;
        }
        if let (Some(a), Some(b)) = (self.to_i128(), o.to_i128()) {
            if let (Some(q), Some(r)) = (a.checked_div(b), a.checked_rem(b)) {
                return Some((Int::from(q), Int::from(r)));
            }
        }
        let (a, b) = (Int::from_mag(self.magnitude(), false), Int::from_mag(o.magnitude(), false));
        let mut q = Int::from(0);
        let mut r = Int::from(0);
        for i in (0..a.active_bits()).rev() {
            r = r.shl(1).or(&Int::from(a.bit(i) as i128));
            q = q.shl(1);
            if r.cmp(&b) != Ordering::Less {
                r = r.sub(&b);
                q = q.or(&Int::from(1));
            }
        }
        let q = if self.is_neg() != o.is_neg() { q.neg() } else { q };
        let r = if self.is_neg() { r.neg() } else { r };
        Some((q, r))
    }

    /// `self` to the power `e`, exact.
    pub fn pow(&self, e: u32) -> Int {
        let mut r = Int::from(1);
        let mut b = self.clone();
        let mut e = e;
        while e > 0 {
            if e & 1 == 1 {
                r = r.mul(&b);
            }
            e >>= 1;
            if e > 0 {
                b = b.mul(&b);
            }
        }
        r
    }

    /// Divides a non-negative value by a small number in place: the remainder.
    fn div_small(m: &mut [u64], d: u64) -> u64 {
        let mut rem = 0u128;
        for x in m.iter_mut().rev() {
            let cur = (rem << 64) | *x as u128;
            *x = (cur / d as u128) as u64;
            rem = cur % d as u128;
        }
        rem as u64
    }

    /// The digits of the magnitude in `radix` (2, 8, 10 or 16), most
    /// significant first; "0" for zero.
    pub fn digits(&self, radix: u32) -> String {
        let mut m = self.magnitude();
        if m.iter().all(|w| *w == 0) {
            return "0".into();
        }
        let mut out: Vec<u8> = Vec::new();
        if radix == 10 {
            const CHUNK: u64 = 10_000_000_000_000_000_000; // 10^19
            while m.iter().any(|w| *w != 0) {
                let mut r = Int::div_small(&mut m, CHUNK);
                let last = m.iter().all(|w| *w == 0);
                for _ in 0..19 {
                    out.push(b'0' + (r % 10) as u8);
                    r /= 10;
                    if last && r == 0 {
                        break;
                    }
                }
            }
        } else {
            let shift = radix.trailing_zeros();
            let v = Int::from_mag(m, false);
            let mut i = 0;
            let bits = v.active_bits();
            while i < bits {
                let d = v.bits(i, shift).to_i128().unwrap_or(0) as u32;
                out.push(std::char::from_digit(d, radix).unwrap() as u8);
                i += shift;
            }
        }
        while out.len() > 1 && *out.last().unwrap() == b'0' {
            out.pop();
        }
        out.reverse();
        String::from_utf8(out).unwrap()
    }

    /// The magnitude divided by 10^k (truncating).
    pub fn div_pow10(&self, k: u32) -> Int {
        let mut m = self.magnitude();
        let mut k = k;
        while k > 0 {
            let step = k.min(19);
            Int::div_small(&mut m, 10u64.pow(step));
            k -= step;
        }
        Int::from_mag(m, false)
    }

    /// Parses digits in base `radix` (2 to 36), ignoring `_`. `None` when a digit is
    /// not valid in that base or there are no digits.
    pub fn parse_radix(digits: &str, radix: u32) -> Option<Int> {
        let mut v = Int::from(0);
        let r = Int::from(radix as i128);
        let mut any = false;
        for c in digits.chars() {
            if c == '_' {
                continue;
            }
            let d = c.to_digit(radix)?;
            any = true;
            v = if radix.is_power_of_two() {
                v.shl(radix.trailing_zeros()).or(&Int::from(d as i128))
            } else {
                v.mul(&r).add(&Int::from(d as i128))
            };
        }
        any.then_some(v)
    }
}

impl PartialOrd for Int {
    fn partial_cmp(&self, o: &Int) -> Option<Ordering> {
        Some(self.cmp(o))
    }
}

impl Ord for Int {
    fn cmp(&self, o: &Int) -> Ordering {
        match (self.is_neg(), o.is_neg()) {
            (true, false) => return Ordering::Less,
            (false, true) => return Ordering::Greater,
            _ => {}
        }
        let n = self.w.len().max(o.w.len());
        for i in (0..n).rev() {
            match self.word(i).cmp(&o.word(i)) {
                Ordering::Equal => {}
                c => return c,
            }
        }
        Ordering::Equal
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn i(v: i128) -> Int {
        Int::from(v)
    }

    #[test]
    fn small_values_match_i128() {
        let vals = [0i128, 1, -1, 7, -8, 1 << 63, -(1 << 63), i64::MAX as i128, 123456789012345678901234567];
        for &a in &vals {
            for &b in &vals {
                assert_eq!(i(a).add(&i(b)).to_i128(), a.checked_add(b), "{a}+{b}");
                assert_eq!(i(a).sub(&i(b)).to_i128(), a.checked_sub(b), "{a}-{b}");
                assert_eq!(i(a).and(&i(b)).to_i128(), Some(a & b));
                assert_eq!(i(a).xor(&i(b)).to_i128(), Some(a ^ b));
                assert_eq!(i(a).cmp(&i(b)), a.cmp(&b), "{a} cmp {b}");
                if b != 0 {
                    if let (Some(q), Some(r)) = (a.checked_div(b), a.checked_rem(b)) {
                        let (qq, rr) = i(a).divrem(&i(b)).unwrap();
                        assert_eq!((qq.to_i128(), rr.to_i128()), (Some(q), Some(r)), "{a}/{b}");
                    }
                }
            }
            assert_eq!(i(a).shr(3).to_i128(), Some(a >> 3));
            assert_eq!(i(a).digits(10), a.unsigned_abs().to_string());
            assert_eq!(i(a).digits(16), format!("{:x}", a.unsigned_abs()));
        }
    }

    #[test]
    fn wide_values() {
        let big = i(1).shl(200).sub(&i(1)); // 200 ones
        assert_eq!(big.active_bits(), 200);
        assert_eq!(big.trunc(8, false).to_i128(), Some(255));
        assert_eq!(big.trunc(8, true).to_i128(), Some(-1));
        assert_eq!(big.bits(190, 20).to_i128(), Some(1023));
        assert_eq!(big.digits(16), "f".repeat(50));
        let ten30 = i(10).pow(30);
        assert_eq!(ten30.digits(10), format!("1{}", "0".repeat(30)));
        let (q, r) = ten30.mul(&ten30).add(&i(7)).divrem(&ten30).unwrap();
        assert_eq!((q, r), (ten30.clone(), i(7)));
        assert_eq!(ten30.mul(&i(-3)).digits(10), format!("3{}", "0".repeat(30)));
        assert!(ten30.mul(&i(-3)).is_neg());
        assert_eq!(Int::parse_radix("1_0000_0000_0000_0000_0000_0000_0000_0000_0000", 16).unwrap(), i(1).shl(144));
        assert_eq!(ten30.div_pow10(28).to_i128(), Some(100));
    }
}
