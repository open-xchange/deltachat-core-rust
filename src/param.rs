use lazy_static::lazy_static;
use regex::*;
use std::collections::BTreeMap;
use std::fmt;
use std::path::PathBuf;
use std::str;

use num_traits::FromPrimitive;
use serde::{Deserialize, Serialize};

use crate::blob::{BlobError, BlobObject};
use crate::context::Context;
use crate::error::{self, bail, ensure};
use crate::message::MsgId;
use crate::mimeparser::SystemMessage;

/// Available param keys.
#[derive(
    PartialEq, Eq, Debug, Clone, Copy, Hash, PartialOrd, Ord, FromPrimitive, Serialize, Deserialize,
)]
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

    /// For Messages: message is encrypted, outgoing: guarantee E2EE or the message is not send
    GuaranteeE2ee = b'c',

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

    /// For Messages
    AttachGroupImage = b'A',

    /// For Messages: space-separated list of messaged IDs of forwarded copies.
    ///
    /// This is used when a [crate::message::Message] is in the
    /// [crate::message::MessageState::OutPending] state but is already forwarded.
    /// In this case the forwarded messages are written to the
    /// database and their message IDs are added to this parameter of
    /// the original message, which is also saved in the database.
    /// When the original message is then finally sent this parameter
    /// is used to also send all the forwarded messages.
    PrepForwards = b'P',

    /// For Jobs
    SetLatitude = b'l',

    /// For Jobs
    SetLongitude = b'n',

    /// For Jobs
    AlsoMove = b'M',

    /// For Jobs: space-separated list of message recipients
    Recipients = b'R',

    /// For Groups
    Unpromoted = b'U',

    /// For Groups and Contacts
    ProfileImage = b'i',

    /// For Chats
    Selftalk = b'K',

    /// For Chats
    Devicetalk = b'D',

    /// For QR
    Auth = b's',

    /// For QR
    GroupId = b'x',

    /// For QR
    GroupName = b'g',

    // For Jobs: space-separated list of keys or key=value pairs
    // CR, LF, ' ', '=' and '\' are escaped as '\r', '\n', '\s', '\e' and '\\', respectively
    Metadata = b'q',

    /// For MDN-sending job
    MsgId = b'I',
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
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
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

    /// Get the given parameter and parse as `bool`.
    pub fn get_bool(&self, key: Param) -> Option<bool> {
        self.get_int(key).map(|v| v != 0)
    }

    /// Get the parameter behind `Param::Cmd` interpreted as `SystemMessage`.
    pub fn get_cmd(&self) -> SystemMessage {
        self.get_int(Param::Cmd)
            .and_then(SystemMessage::from_i32)
            .unwrap_or_default()
    }

    /// Set the parameter behind `Param::Cmd`.
    pub fn set_cmd(&mut self, value: SystemMessage) {
        self.set_int(Param::Cmd, value as i32);
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

    /// Gets the given parameter and parse as [ParamsFile].
    ///
    /// See also [Params::get_blob] and [Params::get_path] which may
    /// be more convenient.
    pub fn get_file<'a>(
        &self,
        key: Param,
        context: &'a Context,
    ) -> Result<Option<ParamsFile<'a>>, BlobError> {
        let val = match self.get(key) {
            Some(val) => val,
            None => return Ok(None),
        };
        ParamsFile::from_param(context, val).map(Some)
    }

    /// Gets the parameter and returns a [BlobObject] for it.
    ///
    /// This parses the parameter value as a [ParamsFile] and than
    /// tries to return a [BlobObject] for that file.  If the file is
    /// not yet a valid blob, one will be created by copying the file
    /// only if `create` is set to `true`, otherwise the a [BlobError]
    /// will result.
    ///
    /// Note that in the [ParamsFile::FsPath] case the blob can be
    /// created without copying if the path already referes to a valid
    /// blob.  If so a [BlobObject] will be returned regardless of the
    /// `create` argument.
    pub fn get_blob<'a>(
        &self,
        key: Param,
        context: &'a Context,
        create: bool,
    ) -> Result<Option<BlobObject<'a>>, BlobError> {
        let val = match self.get(key) {
            Some(val) => val,
            None => return Ok(None),
        };
        let file = ParamsFile::from_param(context, val)?;
        let blob = match file {
            ParamsFile::FsPath(path) => match create {
                true => BlobObject::new_from_path(context, path)?,
                false => BlobObject::from_path(context, path)?,
            },
            ParamsFile::Blob(blob) => blob,
        };
        Ok(Some(blob))
    }

    /// Gets the parameter and returns a [PathBuf] for it.
    ///
    /// This parses the parameter value as a [ParamsFile] and returns
    /// a [PathBuf] to the file.
    pub fn get_path(&self, key: Param, context: &Context) -> Result<Option<PathBuf>, BlobError> {
        let val = match self.get(key) {
            Some(val) => val,
            None => return Ok(None),
        };
        let file = ParamsFile::from_param(context, val)?;
        let path = match file {
            ParamsFile::FsPath(path) => path,
            ParamsFile::Blob(blob) => blob.to_abs_path(),
        };
        Ok(Some(path))
    }

    pub fn get_msg_id(&self) -> Option<MsgId> {
        self.get(Param::MsgId)
            .and_then(|x| x.parse::<u32>().ok())
            .map(MsgId::new)
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

/// The value contained in [Param::File].
///
/// Because the only way to construct this object is from a valid
/// UTF-8 string it is always safe to convert the value contained
/// within the [ParamsFile::FsPath] back to a [String] or [&str].
/// Despite the type itself does not guarantee this.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ParamsFile<'a> {
    FsPath(PathBuf),
    Blob(BlobObject<'a>),
}

impl<'a> ParamsFile<'a> {
    /// Parse the [Param::File] value into an object.
    ///
    /// If the value was stored into the [Params] correctly this
    /// should not fail.
    pub fn from_param(context: &'a Context, src: &str) -> Result<ParamsFile<'a>, BlobError> {
        let param = match src.starts_with("$BLOBDIR/") {
            true => ParamsFile::Blob(BlobObject::from_name(context, src.to_string())?),
            false => ParamsFile::FsPath(PathBuf::from(src)),
        };
        Ok(param)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::fs;
    use std::path::Path;

    use crate::test_utils::*;

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
            .remove(Param::GuaranteeE2ee)
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
        
    fn test_params_file_fs_path() {
        let t = dummy_context();
        if let ParamsFile::FsPath(p) = ParamsFile::from_param(&t.ctx, "/foo/bar/baz").unwrap() {
            assert_eq!(p, Path::new("/foo/bar/baz"));
        } else {
            panic!("Wrong enum variant");
        }
    }

    #[test]
    fn test_params_file_blob() {
        let t = dummy_context();
        if let ParamsFile::Blob(b) = ParamsFile::from_param(&t.ctx, "$BLOBDIR/foo").unwrap() {
            assert_eq!(b.as_name(), "$BLOBDIR/foo");
        } else {
            panic!("Wrong enum variant");
        }
    }

    // Tests for Params::get_file(), Params::get_path() and Params::get_blob().
    #[test]
    fn test_params_get_fileparam() {
        let t = dummy_context();
        let fname = t.dir.path().join("foo");
        let mut p = Params::new();
        p.set(Param::File, fname.to_str().unwrap());

        let file = p.get_file(Param::File, &t.ctx).unwrap().unwrap();
        assert_eq!(file, ParamsFile::FsPath(fname.clone()));

        let path = p.get_path(Param::File, &t.ctx).unwrap().unwrap();
        assert_eq!(path, fname);

        // Blob does not exist yet, expect BlobError.
        let err = p.get_blob(Param::File, &t.ctx, false).unwrap_err();
        match err {
            BlobError::WrongBlobdir { .. } => (),
            _ => panic!("wrong error type/variant: {:?}", err),
        }

        fs::write(fname, b"boo").unwrap();
        let blob = p.get_blob(Param::File, &t.ctx, true).unwrap().unwrap();
        assert_eq!(
            blob,
            BlobObject::from_name(&t.ctx, "foo".to_string()).unwrap()
        );

        // Blob in blobdir, expect blob.
        let bar = t.ctx.get_blobdir().join("bar");
        p.set(Param::File, bar.to_str().unwrap());
        let blob = p.get_blob(Param::File, &t.ctx, false).unwrap().unwrap();
        assert_eq!(
            blob,
            BlobObject::from_name(&t.ctx, "bar".to_string()).unwrap()
        );

        p.remove(Param::File);
        assert!(p.get_file(Param::File, &t.ctx).unwrap().is_none());
        assert!(p.get_path(Param::File, &t.ctx).unwrap().is_none());
        assert!(p.get_blob(Param::File, &t.ctx, false).unwrap().is_none());
    }
}
