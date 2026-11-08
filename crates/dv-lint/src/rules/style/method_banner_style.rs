//! SV-STY-007 (MethodBannerStyle): Standardizes method separator pre-banners.

use crate::support::source_layout::banners;
use crate::syntax::Cst;
use crate::Sink;

pub(crate) fn method_banner(c: &Cst, s: &mut Sink) {
    banners(c, s, true);
}
