/// COI-related overrides for Deltachat.
pub enum CoiDeltachatMode {
    /// "Old" Deltachat behavior. No COI server, COI disabled, or COI message filter set to "none".
    Disabled,

    /// COI enabled and message filter set to "active" or "seen". The server is responsible for
    /// moving message and DeltaChat should be stopped from moving messages.
    /// The mvbox thread should listen for messages in the `mvbox_folder_override` and the inbox
    /// thread should listen for messages in `inbox_folder_override`.
    Enabled {
        inbox_folder_override: String,
        mvbox_folder_override: String,
    },
}

impl CoiDeltachatMode {
    pub fn get_mvbox_folder_override(&self) -> Option<&str> {
        match self {
            Self::Disabled => None,
            Self::Enabled {
                ref mvbox_folder_override,
                ..
            } => Some(mvbox_folder_override),
        }
    }

    pub fn get_inbox_folder_override(&self) -> Option<&str> {
        match self {
            Self::Disabled => None,
            Self::Enabled {
                ref inbox_folder_override,
                ..
            } => Some(inbox_folder_override),
        }
    }

    pub fn mvbox_folder_override_equals(&self, cmp: &str) -> bool {
        match self {
            Self::Enabled {
                ref mvbox_folder_override,
                ..
            } => mvbox_folder_override == cmp,
            _ => false,
        }
    }

    pub fn is_server_side_move_enabled(&self) -> bool {
        match self {
            Self::Disabled => false,
            Self::Enabled { .. } => true,
        }
    }
}
