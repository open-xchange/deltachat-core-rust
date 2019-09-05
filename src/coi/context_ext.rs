use crate::coi::{CoiConfig, CoiDeltachatMode, CoiMessageFilter};
use crate::context::*;
use crate::job::*;
use crate::param::*;

const COI_METADATA_ENABLED: &str = "/private/vendor/vendor.dovecot/coi/config/enabled";
const COI_METADATA_MESSAGE_FILTER: &str =
    "/private/vendor/vendor.dovecot/coi/config/message-filter";

impl Context {
    pub fn get_coi_config(&self) -> Option<CoiConfig> {
        self.inbox.read().unwrap().get_coi_config()
    }

    pub fn set_coi_enabled(&self, enable: bool, id: i32) {
        let value = if enable { "yes" } else { "" };
        let mut params = Params::new();
        params.set_map(Param::Metadata, &[(COI_METADATA_ENABLED, value)]);
        job_add(self, Action::SetMetadata, id as libc::c_int, params, 0);
    }

    pub fn set_coi_message_filter(&self, message_filter: CoiMessageFilter, id: i32) {
        let mut params = Params::new();
        params.set_map(
            Param::Metadata,
            &[(COI_METADATA_MESSAGE_FILTER, &message_filter.to_string())],
        );
        job_add(self, Action::SetMetadata, id as libc::c_int, params, 0);
    }

    pub fn get_coi_message_filter(&self, id: i32) {
        let mut params = Params::new();
        params.set(Param::Metadata, COI_METADATA_MESSAGE_FILTER);
        job_add(self, Action::GetMetadata, id as libc::c_int, params, 0);
    }

    pub fn set_coi_deltachat_mode(&self, new_mode: CoiDeltachatMode) {
        let arc = self.coi_deltachat_mode.clone();
        let mut p = arc.lock().unwrap();
        *p = new_mode;
    }

    pub fn get_mvbox_folder_override(&self) -> Option<String> {
        self.with_coi_deltachat_mode(|mode| {
            mode.get_mvbox_folder_override()
                .map(|mvbox_override| mvbox_override.into())
        })
    }

    pub fn has_mvbox_folder_override(&self) -> bool {
        self.with_coi_deltachat_mode(|mode| mode.get_mvbox_folder_override().is_some())
    }

    /// DCC will move messages depending on two settings:
    ///
    /// * `mvbox_move` has to be enabled (set to "1") in the config, AND
    ///
    /// * `CoiDeltachatMode#is_server_side_move_enabled` has to be set to `false`.
    pub fn is_deltachat_move_enabled(&self) -> bool {
        if self.with_coi_deltachat_mode(|mode| mode.is_server_side_move_enabled()) {
            false
        } else {
            self.sql
                .get_config_int(self, "mvbox_move")
                .map(|value| value == 1)
                .unwrap_or(true)
        }
    }

    /// Helper function that allows us to access the mutex protected `coi_deltachat_mode`
    /// without having to write too verbose code.
    pub fn with_coi_deltachat_mode<T>(&self, cb: impl FnOnce(&CoiDeltachatMode) -> T) -> T {
        let arc = self.coi_deltachat_mode.clone();
        let mode = arc.lock().unwrap();
        cb(&mode)
    }
}
