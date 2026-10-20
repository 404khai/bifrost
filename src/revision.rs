use std::fmt;

use crate::BifrostError;

/// A supported API revision in the fixed, consecutive v1–v4 sequence.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(u32)]
pub enum Revision {
    /// Terminal labels use `name`.
    V1 = 1,
    /// Terminal labels use `title`.
    V2 = 2,
    /// Terminal labels use `label`.
    V3 = 3,
    /// Terminal labels use `display_name`.
    V4 = 4,
}

impl Revision {
    /// Numeric identifier supplied by callers, independent of payload fields.
    pub fn number(self) -> u32 {
        self as u32
    }
}

impl TryFrom<u32> for Revision {
    type Error = BifrostError;

    fn try_from(number: u32) -> Result<Self, Self::Error> {
        match number {
            1 => Ok(Self::V1),
            2 => Ok(Self::V2),
            3 => Ok(Self::V3),
            4 => Ok(Self::V4),
            _ => Err(BifrostError::UnknownRevision { revision: number }),
        }
    }
}

impl fmt::Display for Revision {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "v{}", self.number())
    }
}
