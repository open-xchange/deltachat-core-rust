use strum_macros::{AsRefStr, Display, EnumString};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Display, EnumString, AsRefStr)]
#[strum(serialize_all = "snake_case")]
pub enum CoiMessageFilter {
    None,
    Active,
    Seen,
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
}
