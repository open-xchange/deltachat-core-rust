/// COI-related overrides for Deltachat.
///
/// In case a COI server with message filter "active" or "seen" is present, the server is
/// responsible for moving message. In that case DeltaChat should be stopped from moving messages
/// and the mvbox thread should listen for messages in the `mvbox_folder_override` and the inbox
/// thread should listen for messages in `inbox_folder_override`.
pub struct CoiDeltachatMode {
    pub server_side_move_enabled: bool,
    pub inbox_folder_override: Option<String>,
    pub mvbox_folder_override: Option<String>,
}

impl CoiDeltachatMode {
    pub fn coi_disabled() -> Self {
        Self {
            server_side_move_enabled: false,
            inbox_folder_override: None,
            mvbox_folder_override: None,
        }
    }

    pub fn get_mvbox_folder_override(&self) -> Option<&str> {
        self.mvbox_folder_override.as_ref().map(|s| s.as_ref())
    }

    pub fn get_inbox_folder_override(&self) -> Option<&str> {
        self.inbox_folder_override.as_ref().map(|s| s.as_ref())
    }

    pub fn mvbox_folder_override_equals(&self, cmp: &str) -> bool {
        self.get_mvbox_folder_override()
            .map(|mvbox_folder_override| mvbox_folder_override == cmp)
            .unwrap_or(false)
    }

    pub fn is_server_side_move_enabled(&self) -> bool {
        self.server_side_move_enabled
    }
}

impl Default for CoiDeltachatMode {
    fn default() -> Self {
        Self::coi_disabled()
    }
}
