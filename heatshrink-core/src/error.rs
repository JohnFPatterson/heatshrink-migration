//! Error type for heatshrink-core. Malformed or invalid configuration never panics.

use core::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Error {
    /// Invalid window / lookahead / buffer configuration.
    InvalidConfig,
    /// API misuse (e.g. sink after finish).
    Misuse,
    /// Null-equivalent missing buffer (caller passed empty required slice role).
    NullArg,
    /// Decoder entered an unknown state (should not happen).
    Unknown,
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::InvalidConfig => write!(f, "invalid config"),
            Error::Misuse => write!(f, "api misuse"),
            Error::NullArg => write!(f, "null argument"),
            Error::Unknown => write!(f, "unknown error"),
        }
    }
}

impl std::error::Error for Error {}
