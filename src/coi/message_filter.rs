use std::convert::{From, TryFrom};
use strum_macros::{AsRefStr, Display, EnumString};

// #[derive(Debug, Clone, Copy, PartialEq, Eq, Display)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Display, EnumString, AsRefStr)]
#[strum(serialize_all = "snake_case")]
pub enum CoiMessageFilter {
    None,
    Active,
    Seen,
}

impl Default for CoiMessageFilter {
    fn default() -> Self {
        Self::None
    }
}

impl TryFrom<i32> for CoiMessageFilter {
    type Error = ();

    fn try_from(value: i32) -> Result<Self, <Self as TryFrom<i32>>::Error> {
        match value {
            0 => Ok(CoiMessageFilter::None),
            1 => Ok(CoiMessageFilter::Active),
            2 => Ok(CoiMessageFilter::Seen),
            _ => Err(()),
        }
    }
}

impl From<CoiMessageFilter> for i32 {
    fn from(src: CoiMessageFilter) -> i32 {
        match src {
            CoiMessageFilter::None => 0,
            CoiMessageFilter::Active => 1,
            CoiMessageFilter::Seen => 2,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::convert::From;
    use std::str::FromStr;
    use std::string::ToString;

    #[test]
    fn test_to_string() {
        assert_eq!("none", CoiMessageFilter::None.to_string());
        assert_eq!("active", CoiMessageFilter::Active.to_string());
        assert_eq!("seen", CoiMessageFilter::Seen.to_string());
    }

    #[test]
    fn test_from_string() {
        assert_eq!(
            CoiMessageFilter::None,
            CoiMessageFilter::from_str(&"none").unwrap()
        );
        assert_eq!(
            CoiMessageFilter::Active,
            CoiMessageFilter::from_str(&"active").unwrap()
        );
        assert_eq!(
            CoiMessageFilter::Seen,
            CoiMessageFilter::from_str(&"seen").unwrap()
        );
    }

    #[test]
    fn test_as_int() {
        assert_eq!(i32::from(CoiMessageFilter::None), 0);
        assert_eq!(i32::from(CoiMessageFilter::Active), 1);
        assert_eq!(i32::from(CoiMessageFilter::Seen), 2);
    }

    #[test]
    fn test_from_int() {
        assert_eq!(CoiMessageFilter::try_from(0i32), Ok(CoiMessageFilter::None));
        assert_eq!(
            CoiMessageFilter::try_from(1i32),
            Ok(CoiMessageFilter::Active)
        );
        assert_eq!(CoiMessageFilter::try_from(2i32), Ok(CoiMessageFilter::Seen));
        assert!(CoiMessageFilter::try_from(3i32).is_err());
    }
}
