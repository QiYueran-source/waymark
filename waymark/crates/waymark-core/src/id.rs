//! Stable work-item identity.
//!
//! A stable ID is `wm-` plus 12 Crockford Base32 characters, persisted
//! lowercase and accepted case-insensitively. It is independent of the item's
//! category, title, and path.

use std::fmt;

use serde::{Deserialize, Serialize};

/// Crockford Base32 alphabet: digits plus the letters that survive a
/// transcription error (no `i`, `l`, `o`, `u`).
pub const ALPHABET: &[u8; 32] = b"0123456789abcdefghjkmnpqrstvwxyz";

/// Length of the random part of a stable ID.
pub const BODY_LEN: usize = 12;

/// Prefix carried by every stable ID.
pub const PREFIX: &str = "wm-";

/// A validated, normalized stable ID.
///
/// The only way to build one is through [`WorkItemId::parse`], so a
/// `WorkItemId` value is always lowercase and always grammatically valid.
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct WorkItemId(String);

impl WorkItemId {
    /// Parse an ID literal, accepting any letter case and normalizing to
    /// lowercase. Rejects a missing prefix, a wrong body length, or a
    /// character outside the alphabet.
    pub fn parse(input: &str) -> Result<Self, IdError> {
        let trimmed = input.trim();
        let lower = trimmed.to_ascii_lowercase();
        let body = match lower.strip_prefix(PREFIX) {
            Some(body) => body,
            None => {
                return Err(IdError::MissingPrefix {
                    value: trimmed.to_owned(),
                })
            }
        };
        if body.chars().count() != BODY_LEN {
            return Err(IdError::WrongLength {
                value: trimmed.to_owned(),
                found: body.chars().count(),
            });
        }
        if let Some(bad) = body.chars().find(|c| !ALPHABET.contains(&(*c as u8))) {
            return Err(IdError::BadCharacter {
                value: trimmed.to_owned(),
                character: bad,
            });
        }
        Ok(Self(lower))
    }

    /// Build from an already-validated body (no prefix). Test and migration
    /// helpers use this; callers handling user input must use [`Self::parse`].
    pub fn from_body(body: &str) -> Result<Self, IdError> {
        Self::parse(&format!("{PREFIX}{body}"))
    }

    /// The normalized, on-disk form.
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// The body without the `wm-` prefix.
    pub fn body(&self) -> &str {
        &self.0[PREFIX.len()..]
    }

    /// File stem for this item's document.
    pub fn file_stem(&self) -> &str {
        &self.0
    }

    /// Filename for this item's document.
    pub fn file_name(&self) -> String {
        format!("{}.md", self.0)
    }
}

impl fmt::Display for WorkItemId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl fmt::Debug for WorkItemId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "WorkItemId({})", self.0)
    }
}

impl std::str::FromStr for WorkItemId {
    type Err = IdError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::parse(s)
    }
}

impl TryFrom<String> for WorkItemId {
    type Error = IdError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::parse(&value)
    }
}

impl From<WorkItemId> for String {
    fn from(value: WorkItemId) -> Self {
        value.0
    }
}

/// Why an ID literal was rejected.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum IdError {
    /// The literal does not start with `wm-`.
    #[error("stable ID must start with `{PREFIX}`: {value}")]
    MissingPrefix {
        /// The rejected literal.
        value: String,
    },
    /// The body is not exactly twelve characters.
    #[error("stable ID body must be {BODY_LEN} characters, found {found}: {value}")]
    WrongLength {
        /// The rejected literal.
        value: String,
        /// The observed body length.
        found: usize,
    },
    /// The body contains a character outside Crockford Base32.
    #[error(
        "stable ID contains {character:?}, which is not a Crockford Base32 character: {value}"
    )]
    BadCharacter {
        /// The rejected literal.
        value: String,
        /// The offending character.
        character: char,
    },
}

/// A source of ID bodies, so tests can be deterministic.
pub trait IdSource {
    /// Return the next candidate body (twelve Crockford Base32 characters).
    fn next_candidate(&mut self) -> String;
}

/// Cryptographically random ID bodies for production use.
#[derive(Debug, Default, Clone, Copy)]
pub struct RandomIds;

impl RandomIds {
    /// A new random source.
    pub fn new() -> Self {
        Self
    }
}

impl IdSource for RandomIds {
    fn next_candidate(&mut self) -> String {
        let mut bytes = [0u8; BODY_LEN];
        // `getrandom` only fails when the platform entropy source is broken,
        // which is a program fault rather than a business error.
        getrandom::getrandom(&mut bytes).expect("the operating system entropy source failed");
        bytes
            .iter()
            .map(|byte| ALPHABET[(byte & 0b1_1111) as usize] as char)
            .collect()
    }
}

impl IdSource for Box<dyn IdSource> {
    fn next_candidate(&mut self) -> String {
        (**self).next_candidate()
    }
}

impl IdSource for &mut dyn IdSource {
    fn next_candidate(&mut self) -> String {
        (**self).next_candidate()
    }
}
