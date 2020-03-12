//! # Error handling

use lettre_email::mime;

#[derive(Debug, Fail)]
pub enum Error {
    #[fail(display = "{:?}", _0)]
    Failure(failure::Error),

    #[fail(display = "SQL error: {:?}", _0)]
    SqlError(#[cause] crate::sql::Error),

    #[fail(display = "{:?}", _0)]
    Io(std::io::Error),

    #[fail(display = "{:?}", _0)]
    Message(String),

    #[fail(display = "{:?}", _0)]
    Image(image_meta::ImageError),

    #[fail(display = "{:?}", _0)]
    Utf8(std::str::Utf8Error),

    #[fail(display = "{:?}", _0)]
    Imap(async_imap::error::Error),

    #[fail(display = "PGP: {:?}", _0)]
    Pgp(pgp::errors::Error),

    #[fail(display = "Base64Decode: {:?}", _0)]
    Base64Decode(base64::DecodeError),

    #[fail(display = "{:?}", _0)]
    FromUtf8(std::string::FromUtf8Error),

    #[fail(display = "{}", _0)]
    BlobError(#[cause] crate::blob::BlobError),

    #[fail(display = "Invalid Message ID.")]
    InvalidMsgId,

    #[fail(display = "Watch folder not found {:?}", _0)]
    WatchFolderNotFound(String),

    #[fail(display = "Invalid Email: {:?}", _0)]
    MailParseError(#[cause] mailparse::MailParseError),

    #[fail(display = "Building invalid Email: {:?}", _0)]
    LettreError(#[cause] lettre_email::error::Error),

    #[fail(display = "SMTP error: {:?}", _0)]
    SmtpError(#[cause] async_smtp::error::Error),

    #[fail(display = "FromStr error: {:?}", _0)]
    FromStr(#[cause] mime::FromStrError),

    #[fail(display = "Not Configured")]
    NotConfigured,
}

pub type Result<T> = std::result::Result<T, Error>;

impl From<crate::sql::Error> for Error {
    fn from(err: crate::sql::Error) -> Error {
        Error::SqlError(err)
    }
}

impl From<base64::DecodeError> for Error {
    fn from(err: base64::DecodeError) -> Error {
        Error::Base64Decode(err)
    }
}

impl From<failure::Error> for Error {
    fn from(err: failure::Error) -> Error {
        Error::Failure(err)
    }
}

impl From<std::io::Error> for Error {
    fn from(err: std::io::Error) -> Error {
        Error::Io(err)
    }
}

impl From<std::str::Utf8Error> for Error {
    fn from(err: std::str::Utf8Error) -> Error {
        Error::Utf8(err)
    }
}

impl From<image_meta::ImageError> for Error {
    fn from(err: image_meta::ImageError) -> Error {
        Error::Image(err)
    }
}

impl From<pgp::errors::Error> for Error {
    fn from(err: pgp::errors::Error) -> Error {
        Error::Pgp(err)
    }
}

impl From<std::string::FromUtf8Error> for Error {
    fn from(err: std::string::FromUtf8Error) -> Error {
        Error::FromUtf8(err)
    }
}

impl From<crate::blob::BlobError> for Error {
    fn from(err: crate::blob::BlobError) -> Error {
        Error::BlobError(err)
    }
}

impl From<async_imap::error::Error> for Error {
    fn from(err: async_imap::error::Error) -> Error {
        Error::Imap(err)
    }
}

impl From<crate::message::InvalidMsgId> for Error {
    fn from(_err: crate::message::InvalidMsgId) -> Error {
        Error::InvalidMsgId
    }
}

impl From<mailparse::MailParseError> for Error {
    fn from(err: mailparse::MailParseError) -> Error {
        Error::MailParseError(err)
    }
}

impl From<lettre_email::error::Error> for Error {
    fn from(err: lettre_email::error::Error) -> Error {
        Error::LettreError(err)
    }
}

impl From<mime::FromStrError> for Error {
    fn from(err: mime::FromStrError) -> Error {
        Error::FromStr(err)
    }
}

#[macro_export]
macro_rules! bail {
    ($e:expr) => {
        return Err($crate::error::Error::Message($e.to_string()));
    };
    ($fmt:expr, $($arg:tt)+) => {
        return Err($crate::error::Error::Message(format!($fmt, $($arg)+)));
    };
}

#[macro_export]
macro_rules! format_err {
    ($e:expr) => {
        $crate::error::Error::Message($e.to_string());
    };
    ($fmt:expr, $($arg:tt)+) => {
        $crate::error::Error::Message(format!($fmt, $($arg)+));
    };
}

#[macro_export(local_inner_macros)]
macro_rules! ensure {
    ($cond:expr, $e:expr) => {
        if !($cond) {
            bail!($e);
        }
    };
    ($cond:expr, $fmt:expr, $($arg:tt)+) => {
        if !($cond) {
            bail!($fmt, $($arg)+);
        }
    };
}

#[macro_export]
macro_rules! ensure_eq {
    ($left:expr, $right:expr) => ({
        match (&$left, &$right) {
            (left_val, right_val) => {
                if !(*left_val == *right_val) {
                    bail!(r#"assertion failed: `(left == right)`
  left: `{:?}`,
 right: `{:?}`"#, left_val, right_val)
                }
            }
        }
    });
    ($left:expr, $right:expr,) => ({
        ensure_eq!($left, $right)
    });
    ($left:expr, $right:expr, $($arg:tt)+) => ({
        match (&($left), &($right)) {
            (left_val, right_val) => {
                if !(*left_val == *right_val) {
                    bail!(r#"assertion failed: `(left == right)`
  left: `{:?}`,
 right: `{:?}`: {}"#, left_val, right_val,
                           format_args!($($arg)+))
                }
            }
        }
    });
}
