/// Stores information about the server side moving behavior.
///
/// In case a COI server with message filter "active" or "seen" is present, the server is
/// responsible for moving message. In that case DeltaChat should be stopped from moving messages
/// and the mvbox thread should listen for messages in the `mvbox_folder_override` (COI.Chats).
pub enum ServerSideMove {
    Disabled,
    Enabled { mvbox_folder_override: String },
}

impl ServerSideMove {
    pub fn get_mvbox_folder_override(&self) -> Option<String> {
        match self {
            Self::Enabled {
                ref mvbox_folder_override,
            } => Some(mvbox_folder_override.into()),
            Self::Disabled => None,
        }
    }

    pub fn mvbox_folder_override_equals(&self, cmp: &str) -> bool {
        match self {
            Self::Enabled {
                ref mvbox_folder_override,
            } => mvbox_folder_override == cmp,
            Self::Disabled => false,
        }
    }

    pub fn is_enabled(&self) -> bool {
        match self {
            Self::Enabled { .. } => true,
            Self::Disabled => false,
        }
    }
}
