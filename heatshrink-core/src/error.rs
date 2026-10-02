//! Error type for heatshrink-core. Malformed / invalid API use → `Err`, never panic.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Error {
    NullArg,
    Misuse,
    InvalidConfig,
    Alloc,
    Unknown,
}

pub type Result<T> = core::result::Result<T, Error>;
