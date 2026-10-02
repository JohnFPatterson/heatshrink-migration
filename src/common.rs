pub const AUTHOR: &str = "Scott Vokes <vokes.s@gmail.com>";
pub const URL: &str = "https://github.com/atomicobject/heatshrink";

pub const VERSION_MAJOR: u32 = 0;
pub const VERSION_MINOR: u32 = 4;
pub const VERSION_PATCH: u32 = 1;

pub const MIN_WINDOW_BITS: u8 = 4;
pub const MAX_WINDOW_BITS: u8 = 15;
pub const MIN_LOOKAHEAD_BITS: u8 = 3;

pub const LITERAL_MARKER: u8 = 0x01;
pub const BACKREF_MARKER: u8 = 0x00;

/// Sentinel used when no back-reference match is found (matches C `(uint16_t)-1`).
pub const MATCH_NOT_FOUND: u16 = u16::MAX;

/// Sentinel when the decoder cannot read more bits (matches C `(uint16_t)-1`).
pub const NO_BITS: u16 = u16::MAX;
