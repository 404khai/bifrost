//! Forward-only validation and object-key editing without a JSON value tree.

use std::borrow::Cow;
use std::cmp::Ordering;
use std::collections::BTreeSet;
use std::fmt;

use crate::pointer::escape_token;
use crate::{BifrostError, CompiledChain, Rename, Trie};

/// Resource bounds applied independently to every pass through a document.
#[derive(Debug, Clone, Copy)]
pub struct StreamingLimits {
    pub max_input_bytes: usize,
    pub max_output_bytes: usize,
    /// Number of nested containers; scalars have depth zero. Hard ceiling: 128.
    pub max_depth: usize,
    /// Counts values and object keys, including those in unchanged subtrees.
    pub max_tokens: usize,
    /// Encoded bytes in a string token, including quotes; also bounds new keys.
    pub max_string_bytes: usize,
}

impl Default for StreamingLimits {
    fn default() -> Self {
        Self {
            max_input_bytes: 16 * 1024 * 1024,
            max_output_bytes: 16 * 1024 * 1024,
            max_depth: 64,
            max_tokens: 1_000_000,
            max_string_bytes: 8 * 1024 * 1024,
        }
    }
}

/// Structural counters, not timings or allocation measurements. Counts sum over passes.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct StreamingStats {
    pub passes: usize,
    /// Maximal value subtrees with no applicable trie node.
    pub copied_subtrees: usize,
    /// Encoded bytes in those subtrees, excluding surrounding whitespace.
    pub copied_bytes: usize,
    pub renamed_keys: usize,
}

/// Validated output. No partial output is returned on error.
#[derive(Debug)]
pub struct StreamingOutput {
    pub bytes: Vec<u8>,
    pub stats: StreamingStats,
}

/// Byte offsets are zero-based within the pass that failed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StreamingError {
    InvalidJson { offset: usize, message: String },
    DuplicateKey { offset: usize, key: String },
    LimitExceeded { limit: &'static str },
    InvalidLimits,
    AllocationFailed,
    Translation(BifrostError),
}

impl fmt::Display for StreamingError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidJson { offset, message } => {
                write!(f, "invalid JSON at byte {offset}: {message}")
            }
            Self::DuplicateKey { offset, key } => {
                write!(f, "duplicate object key {key:?} at byte {offset}")
            }
            Self::LimitExceeded { limit } => write!(f, "streaming limit exceeded: {limit}"),
            Self::InvalidLimits => write!(f, "max_depth must not exceed the hard ceiling of 128"),
            Self::AllocationFailed => write!(f, "could not allocate streaming output or edits"),
            Self::Translation(error) => fmt::Display::fmt(error, f),
        }
    }
}

impl std::error::Error for StreamingError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Translation(error) => Some(error),
            _ => None,
        }
    }
}

impl CompiledChain {
    /// Validate and translate JSON bytes using ordered forward-only trie passes.
    /// No full `Value` is built. Empty plans still validate and copy the document.
    /// Duplicate decoded object keys are rejected, even on unchanged paths.
    ///
    /// # Errors
    /// Returns syntax, duplicate-key, limit, allocation, or translation errors.
    /// Each pass is buffered; input remains unchanged and errors return no output.
    ///
    /// ```
    /// use bifrost::{Registry, Revision, StreamingLimits};
    /// let plan = Registry::builtin()?.compile(Revision::V1, Revision::V4)?;
    /// let input = br#"{ "session": {"terminal": {"name": "astrid"}} }"#;
    /// let output = plan.translate_bytes(input, &StreamingLimits::default())?;
    /// assert_eq!(output.bytes,
    ///     br#"{ "session": {"terminal": {"display_name": "astrid"}} }"#);
    /// assert_eq!(output.stats.renamed_keys, 3);
    /// # Ok::<(), Box<dyn std::error::Error>>(())
    /// ```
    pub fn translate_bytes(
        &self,
        input: &[u8],
        limits: &StreamingLimits,
    ) -> Result<StreamingOutput, StreamingError> {
        let Some(first) = self.steps.first() else {
            return run(input, None, limits);
        };
        let mut result = apply_rules_bytes(input, first, limits)?;
        for step in self.steps.iter().skip(1) {
            let next = apply_rules_bytes(&result.bytes, step, limits)?;
            result = StreamingOutput {
                bytes: next.bytes,
                stats: StreamingStats {
                    passes: result.stats.passes.saturating_add(next.stats.passes),
                    copied_subtrees: result
                        .stats
                        .copied_subtrees
                        .saturating_add(next.stats.copied_subtrees),
                    copied_bytes: result
                        .stats
                        .copied_bytes
                        .saturating_add(next.stats.copied_bytes),
                    renamed_keys: result
                        .stats
                        .renamed_keys
                        .saturating_add(next.stats.renamed_keys),
                },
            };
        }
        Ok(result)
    }
}

/// Apply one rule trie to JSON bytes, preserving all bytes except renamed keys.
///
/// Every subtree is validated before copying. Numbers must fit `serde_json`'s
/// default number representation; strings must contain valid Unicode scalars.
/// Only object paths are editable, matching `apply_rules`. Object member order
/// is preserved. Duplicate decoded keys are rejected throughout the document.
/// Syntax/duplicate errors take precedence over translation errors. Translation
/// errors retain the reference engine's sorted-child, descendant-first order.
///
/// # Errors
/// Returns syntax, duplicate-key, resource-limit, allocation, or translation errors.
pub fn apply_rules_bytes(
    input: &[u8],
    rules: &Trie<Rename>,
    limits: &StreamingLimits,
) -> Result<StreamingOutput, StreamingError> {
    run(input, Some(rules), limits)
}

struct Edit {
    start: usize,
    end: usize,
    replacement: Vec<u8>,
}

struct Scanner<'a> {
    input: &'a [u8],
    limits: &'a StreamingLimits,
    pos: usize,
    tokens: usize,
    edits: Vec<Edit>,
    replacement_bytes: usize,
    semantic_error: Option<(Vec<String>, BifrostError)>,
    stats: StreamingStats,
}

fn run(
    input: &[u8],
    rules: Option<&Trie<Rename>>,
    limits: &StreamingLimits,
) -> Result<StreamingOutput, StreamingError> {
    if limits.max_depth > 128 {
        return Err(StreamingError::InvalidLimits);
    }
    check(input.len(), limits.max_input_bytes, "input bytes")?;
    std::str::from_utf8(input).map_err(|error| StreamingError::InvalidJson {
        offset: error.valid_up_to(),
        message: "invalid UTF-8".into(),
    })?;
    let mut scanner = Scanner {
        input,
        limits,
        pos: 0,
        tokens: 0,
        edits: Vec::new(),
        replacement_bytes: 0,
        semantic_error: None,
        stats: StreamingStats {
            passes: 1,
            ..StreamingStats::default()
        },
    };
    scanner.whitespace();
    scanner.value(rules, 0, &mut Vec::new(), true)?;
    scanner.whitespace();
    if scanner.pos != input.len() {
        return Err(scanner.invalid("trailing bytes after JSON value"));
    }
    if let Some((_, error)) = scanner.semantic_error {
        return Err(StreamingError::Translation(error));
    }
    // Compute the complete size before allocating or exposing any output.
    let mut size = input.len();
    for edit in &scanner.edits {
        size = size
            .checked_sub(edit.end - edit.start)
            .and_then(|n| n.checked_add(edit.replacement.len()))
            .ok_or(StreamingError::LimitExceeded {
                limit: "output bytes",
            })?;
    }
    check(size, limits.max_output_bytes, "output bytes")?;
    let mut bytes = Vec::new();
    bytes
        .try_reserve_exact(size)
        .map_err(|_| StreamingError::AllocationFailed)?;
    let mut cursor = 0;
    for edit in scanner.edits {
        bytes.extend_from_slice(&input[cursor..edit.start]);
        bytes.extend_from_slice(&edit.replacement);
        cursor = edit.end;
    }
    bytes.extend_from_slice(&input[cursor..]);
    Ok(StreamingOutput {
        bytes,
        stats: scanner.stats,
    })
}

fn check(actual: usize, maximum: usize, limit: &'static str) -> Result<(), StreamingError> {
    if actual > maximum {
        Err(StreamingError::LimitExceeded { limit })
    } else {
        Ok(())
    }
}

impl<'a> Scanner<'a> {
    fn invalid(&self, message: &str) -> StreamingError {
        StreamingError::InvalidJson {
            offset: self.pos,
            message: message.into(),
        }
    }

    fn peek(&self) -> Option<u8> {
        self.input.get(self.pos).copied()
    }

    fn whitespace(&mut self) {
        while matches!(self.peek(), Some(b' ' | b'\t' | b'\r' | b'\n')) {
            self.pos += 1;
        }
    }

    fn take(&mut self, byte: u8) -> bool {
        if self.peek() == Some(byte) {
            self.pos += 1;
            true
        } else {
            false
        }
    }

    fn token(&mut self) -> Result<(), StreamingError> {
        self.tokens = self.tokens.saturating_add(1);
        check(self.tokens, self.limits.max_tokens, "tokens")
    }

    fn record_error(&mut self, path: &[String], error: BifrostError) {
        let replace = self.semantic_error.as_ref().is_none_or(|(old, _)| {
            // Postorder: children sort before their parent; siblings sort by token.
            let common = path.iter().zip(old).find(|(a, b)| a != b);
            let ordering = match common {
                Some((a, b)) => a.cmp(b),
                None => old.len().cmp(&path.len()),
            };
            ordering == Ordering::Less
        });
        if replace {
            self.semantic_error = Some((path.to_vec(), error));
        }
    }

    fn path(path: &[String]) -> String {
        path.iter()
            .map(|token| format!("/{}", escape_token(token)))
            .collect()
    }

    fn value(
        &mut self,
        node: Option<&Trie<Rename>>,
        depth: usize,
        path: &mut Vec<String>,
        count_copy: bool,
    ) -> Result<(), StreamingError> {
        self.token()?;
        let node = node.filter(|node| node.has_value_or_children());
        let relevant = node.is_some();
        let start = self.pos;
        if relevant && self.peek() != Some(b'{') {
            self.record_error(
                path,
                BifrostError::ExpectedObject {
                    path: Self::path(path),
                },
            );
        }
        match self.peek() {
            Some(b'{') => {
                check(depth + 1, self.limits.max_depth, "depth")?;
                self.object(node, depth + 1, path)?;
            }
            Some(b'[') => {
                check(depth + 1, self.limits.max_depth, "depth")?;
                self.pos += 1;
                self.whitespace();
                if !self.take(b']') {
                    loop {
                        // Rules through arrays remain unsupported, as in the oracle.
                        self.value(None, depth + 1, path, false)?;
                        self.whitespace();
                        if self.take(b']') {
                            break;
                        }
                        if !self.take(b',') {
                            return Err(self.invalid("expected , or ]"));
                        }
                        self.whitespace();
                    }
                }
            }
            Some(b'"') => {
                self.string()?;
            }
            Some(b't') => self.literal(b"true")?,
            Some(b'f') => self.literal(b"false")?,
            Some(b'n') => self.literal(b"null")?,
            Some(b'-' | b'0'..=b'9') => self.number()?,
            _ => return Err(self.invalid("expected JSON value")),
        }
        if !relevant && count_copy {
            self.stats.copied_subtrees += 1;
            self.stats.copied_bytes += self.pos - start;
        }
        Ok(())
    }

    fn object(
        &mut self,
        node: Option<&Trie<Rename>>,
        depth: usize,
        path: &mut Vec<String>,
    ) -> Result<(), StreamingError> {
        self.pos += 1;
        self.whitespace();
        let mut keys: BTreeSet<Cow<'a, str>> = BTreeSet::new();
        if !self.take(b'}') {
            loop {
                self.token()?;
                let start = self.pos;
                self.string()?;
                let end = self.pos;
                // Profiling showed key decoding and cloning dominated allocation
                // counts. Validated keys without escapes can borrow input bytes;
                // escaped keys still need an owned decoded spelling for equality.
                let encoded: &'a [u8] = &self.input[start + 1..end - 1];
                let key = if encoded.contains(&b'\\') {
                    Cow::Owned(
                        serde_json::from_slice::<String>(&self.input[start..end])
                            .map_err(|error| self.invalid(&error.to_string()))?,
                    )
                } else {
                    Cow::Borrowed(
                        std::str::from_utf8(encoded)
                            .map_err(|error| self.invalid(&error.to_string()))?,
                    )
                };
                if !keys.insert(key.clone()) {
                    return Err(StreamingError::DuplicateKey {
                        offset: start,
                        key: key.into_owned(),
                    });
                }
                if let Some(rule) = node.and_then(|node| node.value.as_ref())
                    && key.as_ref() == rule.source.as_str()
                    && rule.source != rule.destination
                {
                    self.edit(start, end, &rule.destination)?;
                }
                self.whitespace();
                if !self.take(b':') {
                    return Err(self.invalid("expected :"));
                }
                self.whitespace();
                let child = node.and_then(|node| node.children.get(key.as_ref()));
                // Unrelated keys need no path allocation for semantic diagnostics.
                if child.is_some() {
                    path.push(key.into_owned());
                }
                self.value(child, depth, path, node.is_some())?;
                if child.is_some() {
                    path.pop();
                }
                self.whitespace();
                if self.take(b'}') {
                    break;
                }
                if !self.take(b',') {
                    return Err(self.invalid("expected , or }"));
                }
                self.whitespace();
            }
        }
        if let Some(rule) = node.and_then(|node| node.value.as_ref())
            && rule.source != rule.destination
            && keys.contains(rule.source.as_str())
            && keys.contains(rule.destination.as_str())
        {
            self.record_error(
                path,
                BifrostError::Conflict {
                    path: Self::path(path),
                    source: rule.source.clone(),
                    destination: rule.destination.clone(),
                },
            );
        }
        Ok(())
    }

    fn edit(&mut self, start: usize, end: usize, destination: &str) -> Result<(), StreamingError> {
        // Exact encoded length bounds allocation before serializing a trusted rule key.
        let size = destination
            .bytes()
            .try_fold(2usize, |size, byte| {
                let width = match byte {
                    b'"' | b'\\' | 8 | 9 | 10 | 12 | 13 => 2,
                    0..=31 => 6,
                    _ => 1,
                };
                size.checked_add(width)
            })
            .ok_or(StreamingError::LimitExceeded {
                limit: "string bytes",
            })?;
        check(size, self.limits.max_string_bytes, "string bytes")?;
        check(size, self.limits.max_output_bytes, "output bytes")?;
        self.replacement_bytes =
            self.replacement_bytes
                .checked_add(size)
                .ok_or(StreamingError::LimitExceeded {
                    limit: "output bytes",
                })?;
        check(
            self.replacement_bytes,
            self.limits.max_output_bytes,
            "output bytes",
        )?;
        let replacement =
            serde_json::to_vec(destination).map_err(|error| self.invalid(&error.to_string()))?;
        self.edits
            .try_reserve(1)
            .map_err(|_| StreamingError::AllocationFailed)?;
        self.edits.push(Edit {
            start,
            end,
            replacement,
        });
        self.stats.renamed_keys += 1;
        Ok(())
    }

    fn literal(&mut self, literal: &[u8]) -> Result<(), StreamingError> {
        if self.input[self.pos..].starts_with(literal) {
            self.pos += literal.len();
            Ok(())
        } else {
            Err(self.invalid("invalid literal"))
        }
    }

    fn number(&mut self) -> Result<(), StreamingError> {
        let start = self.pos;
        while matches!(
            self.peek(),
            Some(b'0'..=b'9' | b'-' | b'+' | b'.' | b'e' | b'E')
        ) {
            self.pos += 1;
        }
        // Leaf-only numeric validation keeps range behavior aligned with the oracle.
        serde_json::from_slice::<serde_json::Number>(&self.input[start..self.pos])
            .map_err(|error| self.invalid(&error.to_string()))?;
        Ok(())
    }

    fn string(&mut self) -> Result<(), StreamingError> {
        let start = self.pos;
        if !self.take(b'"') {
            return Err(self.invalid("expected string"));
        }
        loop {
            check(
                self.pos - start,
                self.limits.max_string_bytes,
                "string bytes",
            )?;
            match self.peek() {
                Some(b'"') => {
                    self.pos += 1;
                    return check(
                        self.pos - start,
                        self.limits.max_string_bytes,
                        "string bytes",
                    );
                }
                Some(b'\\') => {
                    self.pos += 1;
                    match self.peek() {
                        Some(b'"' | b'\\' | b'/' | b'b' | b'f' | b'n' | b'r' | b't') => {
                            self.pos += 1
                        }
                        Some(b'u') => {
                            self.pos += 1;
                            let code = self.hex_quad()?;
                            if (0xD800..=0xDBFF).contains(&code) {
                                if !self.take(b'\\') || !self.take(b'u') {
                                    return Err(self.invalid("expected low surrogate"));
                                }
                                let low = self.hex_quad()?;
                                if !(0xDC00..=0xDFFF).contains(&low) {
                                    return Err(self.invalid("invalid low surrogate"));
                                }
                            } else if (0xDC00..=0xDFFF).contains(&code) {
                                return Err(self.invalid("unpaired low surrogate"));
                            }
                        }
                        _ => return Err(self.invalid("invalid string escape")),
                    }
                }
                Some(0..=31) => return Err(self.invalid("unescaped control character")),
                Some(_) => self.pos += 1,
                None => return Err(self.invalid("unterminated string")),
            }
        }
    }

    fn hex_quad(&mut self) -> Result<u16, StreamingError> {
        let mut value = 0;
        for _ in 0..4 {
            let digit = match self.peek() {
                Some(b'0'..=b'9') => self.input[self.pos] - b'0',
                Some(b'a'..=b'f') => self.input[self.pos] - b'a' + 10,
                Some(b'A'..=b'F') => self.input[self.pos] - b'A' + 10,
                _ => return Err(self.invalid("invalid Unicode escape")),
            };
            self.pos += 1;
            value = value * 16 + u16::from(digit);
        }
        Ok(value)
    }
}
