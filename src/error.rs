use failure::Fail;

#[derive(Debug, Fail)]
pub enum Error {
    #[fail(display = "Sqlite Error: {:?}", _0)]
    Sql(rusqlite::Error),
    #[fail(display = "Sqlite Connection Pool Error: {:?}", _0)]
    ConnectionPool(r2d2::Error),
    #[fail(display = "{:?}", _0)]
    Failure(failure::Error),
    #[fail(display = "Sqlite: Connection closed")]
    SqlNoConnection,
    #[fail(display = "Sqlite: Already open")]
    SqlAlreadyOpen,
    #[fail(display = "Sqlite: Failed to open")]
    SqlFailedToOpen,
    #[fail(display = "{:?}", _0)]
    Io(std::io::Error),
    #[fail(display = "{:?}", _0)]
    Message(String),
    #[fail(display = "{:?}", _0)]
    Image(image_meta::ImageError),
    #[fail(display = "{:?}", _0)]
    Utf8(std::str::Utf8Error),
    #[fail(display = "{:?}", _0)]
    Imap(imap::error::Error),
    CStringError(crate::dc_tools::CStringError),
    #[fail(display = "PGP: {:?}", _0)]
    Pgp(pgp::errors::Error),
    #[fail(display = "Base64Decode: {:?}", _0)]
    Base64Decode(base64::DecodeError),
    #[fail(display = "{:?}", _0)]
    FromUtf8(std::string::FromUtf8Error),
}

pub type Result<T> = std::result::Result<T, Error>;

impl From<rusqlite::Error> for Error {
    fn from(err: rusqlite::Error) -> Error {
        Error::Sql(err)
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

impl From<r2d2::Error> for Error {
    fn from(err: r2d2::Error) -> Error {
        Error::ConnectionPool(err)
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

impl From<std::string::FromUtf8Error> for Error {
    fn from(err: std::string::FromUtf8Error) -> Error {
        Error::FromUtf8(err)
    }
}

impl From<image_meta::ImageError> for Error {
    fn from(err: image_meta::ImageError) -> Error {
        Error::Image(err)
    }
}

impl From<imap::error::Error> for Error {
    fn from(err: imap::error::Error) -> Error {
        Error::Imap(err)
    }
}

impl From<crate::dc_tools::CStringError> for Error {
    fn from(err: crate::dc_tools::CStringError) -> Error {
        Error::CStringError(err)
    }
}

impl From<pgp::errors::Error> for Error {
    fn from(err: pgp::errors::Error) -> Error {
        Error::Pgp(err)
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
