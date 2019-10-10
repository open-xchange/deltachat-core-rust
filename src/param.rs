use lazy_static::lazy_static;
use regex::*;
use std::collections::BTreeMap;
use std::fmt;
use std::str;

use num_traits::FromPrimitive;

use crate::dc_mimeparser::SystemMessage;
use crate::error;

/// Available param keys.
#[derive(PartialEq, Eq, Debug, Clone, Copy, Hash, PartialOrd, Ord, FromPrimitive)]
#[repr(u8)]
pub enum Param {
    /// For messages and jobs
    File = b'f',
    /// For Messages
    Width = b'w',
    /// For Messages
    Height = b'h',
    /// For Messages
    Duration = b'd',
    /// For Messages
    MimeType = b'm',
    /// For Messages: message is encryoted, outgoing: guarantee E2EE or the message is not send
    GuranteeE2ee = b'c',
    /// For Messages: decrypted with validation errors or without mutual set, if neither
    /// 'c' nor 'e' are preset, the messages is only transport encrypted.
    ErroneousE2ee = b'e',
    /// For Messages: force unencrypted message, either `ForcePlaintext::AddAutocryptHeader` (1),
    /// `ForcePlaintext::NoAutocryptHeader` (2) or 0.
    ForcePlaintext = b'u',
    /// For Messages
    WantsMdn = b'r',
    /// For Messages
    Forwarded = b'a',
    /// For Messages
    Cmd = b'S',
    /// For Messages
    Arg = b'E',
    /// For Messages
    Arg2 = b'F',
    /// For Messages
    Arg3 = b'G',
    /// For Messages
    Arg4 = b'H',
    /// For Messages
    Error = b'L',
    /// For Messages: space-separated list of messaged IDs of forwarded copies.
    PrepForwards = b'P',
    /// For Jobs
    SetLatitude = b'l',
    /// For Jobs
    SetLongitude = b'n',
    /// For Jobs
    ServerFolder = b'Z',
    /// For Jobs
    ServerUid = b'z',
    /// For Jobs
    AlsoMove = b'M',
    /// For Jobs: space-separated list of message recipients
    Recipients = b'R',
    // For Groups
    Unpromoted = b'U',
    // For Groups and Contacts
    ProfileImage = b'i',
    // For Chats
    Selftalk = b'K',
    // For QR
    Auth = b's',
    // For QR
    GroupId = b'x',
    // For QR
    GroupName = b'g',
    // For Jobs: space-separated list of keys or key=value pairs
    // CR, LF, ' ', '=' and '\' are escaped as '\r', '\n', '\s', '\e' and '\\', respectively
    Metadata = b'D',
}

/// Possible values for `Param::ForcePlaintext`.
#[derive(PartialEq, Eq, Debug, Clone, Copy, FromPrimitive)]
#[repr(u8)]
pub enum ForcePlaintext {
    AddAutocryptHeader = 1,
    NoAutocryptHeader = 2,
}

/// An object for handling key=value parameter lists.
///
/// The structure is serialized by calling `to_string()` on it.
///
/// Only for library-internal use.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Params {
    inner: BTreeMap<Param, String>,
}

impl fmt::Display for Params {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for (i, (key, value)) in self.inner.iter().enumerate() {
            if i > 0 {
                writeln!(f)?;
            }
            write!(f, "{}={}", *key as u8 as char, value)?;
        }
        Ok(())
    }
}

impl str::FromStr for Params {
    type Err = error::Error;

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        let mut inner = BTreeMap::new();
        for pair in s.trim().lines() {
            let pair = pair.trim();
            if pair.is_empty() {
                continue;
            }
            // TODO: probably nicer using a regex
            ensure!(pair.len() > 1, "Invalid key pair: '{}'", pair);
            let mut split = pair.splitn(2, '=');
            let key = split.next();
            let value = split.next();

            ensure!(key.is_some(), "Missing key");
            ensure!(value.is_some(), "Missing value");

            let key = key.unwrap_or_default().trim();
            let value = value.unwrap_or_default().trim();

            if let Some(key) = Param::from_u8(key.as_bytes()[0]) {
                inner.insert(key, value.to_string());
            } else {
                bail!("Unknown key: {}", key);
            }
        }

        Ok(Params { inner })
    }
}

pub fn escape_param(s: &str) -> String {
    lazy_static! { static ref RE: Regex = Regex::new(r"[\n\r =\\]").unwrap(); }
    RE.replace_all(s, |c: &Captures| match &c[0] {
        "\n" => "\\n",
        "\r" => "\\r",
        " " => "\\s",
        "=" => "\\e",
        "\\" => "\\\\",
        _ => "",
    }).to_string()
}

pub fn unescape_param(s: &str) -> String {
    lazy_static! { static ref RE: Regex = Regex::new(r"\\[nrse\\]").unwrap(); }
    RE.replace_all(s, |c: &Captures| match &c[0] {
        "\\n" => "\n",
        "\\r" => "\r",
        "\\s" => " ",
        "\\e" => "=",
        "\\\\" => "\\",
        _ => unreachable!(),
    }).to_string()
}

impl Params {
    /// Create new empty params.
    pub fn new() -> Self {
        Default::default()
    }

    /// Get the value of the given key, return `None` if no value is set.
    pub fn get(&self, key: Param) -> Option<&str> {
        self.inner.get(&key).map(|s| s.as_str())
    }

    /// Check if the given key is set.
    pub fn exists(&self, key: Param) -> bool {
        self.inner.contains_key(&key)
    }

    /// Set the given key to the passed in value.
    pub fn set(&mut self, key: Param, value: impl AsRef<str>) -> &mut Self {
        self.inner.insert(key, value.as_ref().to_string());
        self
    }

    /// Removes the given key, if it exists.
    pub fn remove(&mut self, key: Param) -> &mut Self {
        self.inner.remove(&key);
        self
    }

    /// Check if there are any values in this.
    pub fn is_empty(&self) -> bool {
        self.inner.is_empty()
    }

    /// Returns how many key-value pairs are set.
    pub fn len(&self) -> usize {
        self.inner.len()
    }

    /// Get the given parameter and parse as `i32`.
    pub fn get_int(&self, key: Param) -> Option<i32> {
        self.get(key).and_then(|s| s.parse().ok())
    }

    /// Get the parameter behind `Param::Cmd` interpreted as `SystemMessage`.
    pub fn get_cmd(&self) -> SystemMessage {
        self.get_int(Param::Cmd)
            .and_then(SystemMessage::from_i32)
            .unwrap_or_default()
    }

    /// Get the given parameter and parse as `f64`.
    pub fn get_float(&self, key: Param) -> Option<f64> {
        self.get(key).and_then(|s| s.parse().ok())
    }

    /// Get the given parameter and parse as a space-separated list
    /// of escaped strings.
    pub fn get_list(&self, key: Param) -> Option<Vec<String>> {
        self.get(key).map(|s| s.split(' ').map(unescape_param).collect())
    }

    /// Get the given parameter and parse as a space-separated list
    /// of escaped `key=value` pairs.
    pub fn get_map(&self, key: Param) -> Option<Vec<(String, String)>> {
        Some(self.get(key)?.split(' ').filter_map(|s| {
            let mut pair = s.splitn(2, '=').map(unescape_param);
            Some((pair.next()?, pair.next()?))
        }).collect())
    }

    /// Set the given paramter to the passed in `i32`.
    pub fn set_int(&mut self, key: Param, value: i32) -> &mut Self {
        self.set(key, format!("{}", value));
        self
    }

    /// Set the given parameter to the passed in `f64` .
    pub fn set_float(&mut self, key: Param, value: f64) -> &mut Self {
        self.set(key, format!("{}", value));
        self
    }

    /// Set the given parameter to the passed in list of strings
    pub fn set_list(&mut self, key: Param, value: &[&str]) -> &mut Self {
        self.set(key, value.iter().map(|s| escape_param(s)).collect::<Vec<_>>().join(" "));
        self
    }

    /// Set the given parameter to the passed in list of `(key, value)` pairs
    pub fn set_map<'a>(&mut self, key: Param, value: &[(&str, &str)]) -> &mut Self {
        self.set(key, value.iter().map(|(k, v)| format!("{}={}", escape_param(k), escape_param(v))).collect::<Vec<_>>().join(" "));
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_dc_param() {
        let mut p1: Params = "\r\n\r\na=1\nf=2\n\nc = 3 ".parse().unwrap();

        assert_eq!(p1.get_int(Param::Forwarded), Some(1));
        assert_eq!(p1.get_int(Param::File), Some(2));
        assert_eq!(p1.get_int(Param::Height), None);
        assert!(!p1.exists(Param::Height));

        p1.set_int(Param::Duration, 4);

        assert_eq!(p1.get_int(Param::Duration), Some(4));

        let mut p1 = Params::new();

        p1.set(Param::Forwarded, "foo")
            .set_int(Param::File, 2)
            .remove(Param::GuranteeE2ee)
            .set_int(Param::Duration, 4);

        assert_eq!(p1.to_string(), "a=foo\nd=4\nf=2");

        p1.remove(Param::File);

        assert_eq!(p1.to_string(), "a=foo\nd=4",);
        assert_eq!(p1.len(), 2);

        p1.remove(Param::Forwarded);
        p1.remove(Param::Duration);

        assert_eq!(p1.to_string(), "",);

        assert!(p1.is_empty());
        assert_eq!(p1.len(), 0)
    }

    #[test]
    fn test_regression() {
        let p1: Params = "a=cli%40deltachat.de\nn=\ni=TbnwJ6lSvD5\ns=0ejvbdFSQxB"
            .parse()
            .unwrap();
        assert_eq!(p1.get(Param::Forwarded).unwrap(), "cli%40deltachat.de");
    }

    #[test]
    fn test_escape_param() {
        assert_eq!(escape_param("test key=value\\s"), "test\\skey\\evalue\\\\s");
    }

    #[test]
    fn test_unescape_param() {
        assert_eq!(unescape_param("\\n"), "\n");
        assert_eq!(unescape_param("\\\\"), "\\");
        assert_eq!(unescape_param("\\a\\"), "\\a\\");
        assert_eq!(unescape_param("test\\skey\\evalue\\\\s"), "test key=value\\s");
        assert_eq!(unescape_param("test key=value\\x"), "test key=value\\x");
    }
}
