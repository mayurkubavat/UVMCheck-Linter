//! SV-STY-008 (SectionHeaderBanner): Standardizes major section divider banners
//! to uniform 70–80 character widths.

use crate::syntax::Cst;
use crate::Sink;
use crate::support::source_layout::{banners};

pub(crate) fn section_banner(c: &Cst, s: &mut Sink) {
    banners(c, s, false);
}
