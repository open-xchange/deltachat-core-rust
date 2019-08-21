use crate::error::Error;
use std::convert::TryFrom;
use std::default::Default;

/// Specifies how incoming chat messages should be filtered.
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum FilterMode {
    /// Do not move chat messages out of the Inbox.
    None,

    /// Let Deltachat move chat messages from Inbox to a configured folder. Requires no server-side
    /// COI support. This is the default behaviour.
    Deltachat,

    /// Let the COI server move chat messages to the standard COI/Chats folder.
    CoiActive,

    /// Let the COI server move chat messages to the standard COI/Chats folder after they have been
    /// marked as seen.
    CoiMoveAfterRead,
}

impl Default for FilterMode {
    fn default() -> Self {
        Self::Deltachat
    }
}

impl TryFrom<&str> for FilterMode {
    type Error = Error;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        match value {
            "0" => Ok(Self::None),
            "1" => Ok(Self::Deltachat),
            "2" => Ok(Self::CoiActive),
            "3" => Ok(Self::CoiMoveAfterRead),
            _ => Err(format_err!("Unsupported FilterMode: {}", value).into()),
        }
    }
}

impl AsRef<str> for FilterMode {
    fn as_ref(&self) -> &str {
        match self {
            Self::None => "0",
            Self::Deltachat => "1",
            Self::CoiActive => "2",
            Self::CoiMoveAfterRead => "3",
        }
    }
}

pub fn get_filter_mode(context: &crate::context::Context) -> FilterMode {
    context
        .sql
        .get_config(context, "mvbox_move")
        .and_then(|s| FilterMode::try_from(s.as_str()).ok())
        .unwrap_or_default()
}
