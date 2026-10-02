pub const HEATSHRINK_AUTHOR: &str = "Scott Vokes <vokes.s@gmail.com>";
pub const HEATSHRINK_URL: &str = "https://github.com/atomicobject/heatshrink";

pub const HEATSHRINK_VERSION_MAJOR: u8 = 0;
pub const HEATSHRINK_VERSION_MINOR: u8 = 4;
pub const HEATSHRINK_VERSION_PATCH: u8 = 1;

pub const HEATSHRINK_MIN_WINDOW_BITS: u8 = 4;
pub const HEATSHRINK_MAX_WINDOW_BITS: u8 = 15;
pub const HEATSHRINK_MIN_LOOKAHEAD_BITS: u8 = 3;

pub const HEATSHRINK_LITERAL_MARKER: u8 = 0x01;
pub const HEATSHRINK_BACKREF_MARKER: u8 = 0x00;

pub(crate) const MATCH_NOT_FOUND: u16 = u16::MAX;

pub(crate) fn valid_window_lookahead(window_sz2: u8, lookahead_sz2: u8) -> bool {
    (HEATSHRINK_MIN_WINDOW_BITS..=HEATSHRINK_MAX_WINDOW_BITS).contains(&window_sz2)
        && (lookahead_sz2 >= HEATSHRINK_MIN_LOOKAHEAD_BITS)
        && (lookahead_sz2 < window_sz2)
}
