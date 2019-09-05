use strum_macros::{AsRefStr, Display, EnumString};
use std::convert::TryFrom;

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

#[cfg(test)]
mod tests {
    use super::*;
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
        assert_eq!(CoiMessageFilter::None   as i32, 0);
        assert_eq!(CoiMessageFilter::Active as i32, 1);
        assert_eq!(CoiMessageFilter::Seen   as i32, 2);
    }

    #[test]
    fn test_from_int() {
        assert_eq!(0 as CoiMessageFilter, CoiMessageFilter::None);
        assert_eq!(1 as CoiMessageFilter, CoiMessageFilter::Active);
        assert_eq!(2 as CoiMessageFilter, CoiMessageFilter::Seen);
    }
}
