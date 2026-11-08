//! SV-STY-007 (MethodBannerStyle): Standardizes method separator pre-banners.

use crate::syntax::Cst;
use crate::Sink;
use crate::support::source_layout::{banners};

pub(crate) fn method_banner(c: &Cst, s: &mut Sink) {
    banners(c, s, true);
}
