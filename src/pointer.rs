//! JSON Pointer string syntax (RFC 6901), independent of document evaluation.

use std::fmt;

/// Decoded JSON Pointer tokens. Numeric tokens remain strings.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Pointer {
    tokens: Vec<String>,
}

/// Invalid JSON Pointer string syntax.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PointerError {
    /// A nonempty pointer must begin with `/`.
    MissingLeadingSlash,
    /// Only `~0` and `~1` are valid escapes. Token indices start at zero.
    InvalidEscape { token_index: usize },
}

impl fmt::Display for PointerError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MissingLeadingSlash => write!(f, "a nonempty JSON Pointer must start with /"),
            Self::InvalidEscape { token_index } => {
                write!(
                    f,
                    "invalid JSON Pointer escape in token {token_index}: expected ~0 or ~1"
                )
            }
        }
    }
}

impl std::error::Error for PointerError {}

impl Pointer {
    /// Parse an already decoded Rust string, not a JSON literal or URI fragment.
    /// Empty input denotes the root; `/` denotes one empty token.
    ///
    /// # Errors
    /// Returns an error for missing leading `/` or invalid/dangling `~` escapes.
    pub fn parse(input: &str) -> Result<Self, PointerError> {
        if input.is_empty() {
            return Ok(Self { tokens: Vec::new() });
        }
        let rest = input
            .strip_prefix('/')
            .ok_or(PointerError::MissingLeadingSlash)?;
        let mut tokens = Vec::new();
        for (token_index, raw) in rest.split('/').enumerate() {
            let mut token = String::new();
            let mut chars = raw.chars();
            while let Some(ch) = chars.next() {
                match ch {
                    '~' => match chars.next() {
                        Some('0') => token.push('~'),
                        Some('1') => token.push('/'),
                        _ => return Err(PointerError::InvalidEscape { token_index }),
                    },
                    _ => token.push(ch),
                }
            }
            tokens.push(token);
        }
        Ok(Self { tokens })
    }

    /// Decoded tokens; root is an empty slice, not a slice with an empty string.
    pub fn tokens(&self) -> &[String] {
        &self.tokens
    }
}

impl fmt::Display for Pointer {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for token in &self.tokens {
            write!(f, "/{}", escape_token(token))?;
        }
        Ok(())
    }
}

pub(crate) fn escape_token(token: &str) -> String {
    token.replace('~', "~0").replace('/', "~1")
}
