//! Byte-preserving text returned by tmux.

use std::borrow::Cow;
use std::ffi::OsString;
use std::fmt;
use std::result::Result as StdResult;
use std::str::Utf8Error;

/// Text returned by tmux without an implicit UTF-8 conversion.
#[derive(Clone, Default, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct TmuxText(Vec<u8>);

impl TmuxText {
    /// Creates text from raw bytes.
    #[must_use]
    pub const fn new(bytes: Vec<u8>) -> Self {
        Self(bytes)
    }

    /// Returns the raw bytes.
    #[must_use]
    pub fn as_bytes(&self) -> &[u8] {
        &self.0
    }

    /// Returns a UTF-8 view when the bytes are valid UTF-8.
    pub fn to_str(&self) -> StdResult<&str, Utf8Error> {
        std::str::from_utf8(&self.0)
    }

    /// Returns a lossy UTF-8 view.
    #[must_use]
    pub fn to_string_lossy(&self) -> Cow<'_, str> {
        String::from_utf8_lossy(&self.0)
    }

    /// Returns whether the text has no bytes.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// Consumes the wrapper and returns the raw bytes.
    #[must_use]
    pub fn into_bytes(self) -> Vec<u8> {
        self.0
    }

    pub(crate) fn to_os_string(&self) -> OsString {
        #[cfg(unix)]
        {
            use std::os::unix::ffi::OsStringExt;
            OsString::from_vec(self.0.clone())
        }
        #[cfg(not(unix))]
        {
            OsString::from(self.to_string_lossy().into_owned())
        }
    }
}

impl AsRef<[u8]> for TmuxText {
    fn as_ref(&self) -> &[u8] {
        self.as_bytes()
    }
}

impl From<&str> for TmuxText {
    fn from(value: &str) -> Self {
        Self(value.as_bytes().to_vec())
    }
}

impl From<String> for TmuxText {
    fn from(value: String) -> Self {
        Self(value.into_bytes())
    }
}

impl From<Vec<u8>> for TmuxText {
    fn from(value: Vec<u8>) -> Self {
        Self(value)
    }
}

impl fmt::Debug for TmuxText {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_tuple("TmuxText")
            .field(&self.to_string_lossy())
            .finish()
    }
}

impl fmt::Display for TmuxText {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.to_string_lossy())
    }
}

#[cfg(test)]
mod tests {
    use super::TmuxText;

    #[test]
    fn invalid_utf8_is_preserved() {
        let text = TmuxText::new(vec![0xff, b'a']);
        assert!(text.to_str().is_err());
        assert_eq!(text.as_bytes(), &[0xff, b'a']);
    }
}
