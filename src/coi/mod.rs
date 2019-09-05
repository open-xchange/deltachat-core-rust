pub mod config;
pub mod message_filter;

use crate::context::*;
use crate::job::*;
use crate::param::*;
pub use crate::server_side_move::ServerSideMove;
pub use config::CoiConfig;
pub use message_filter::CoiMessageFilter;

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

    pub fn set_server_side_move_config(&self, new_config: ServerSideMove) {
        let arc = self.server_side_move_config.clone();
        let mut ssm = arc.lock().unwrap();
        *ssm = new_config;
    }

    pub fn get_mvbox_folder_override(&self) -> Option<String> {
        self.with_server_side_move_config(|config| config.get_mvbox_folder_override())
    }

    /// DCC will move messages depending on two settings:
    ///
    /// * `mvbox_move` has to be enabled (set to "1") in the config.
    ///
    /// * `server_side_move_config` has to be set to ServerSideMove::Disabled.
    pub fn is_deltachat_move_enabled(&self) -> bool {
        if self.with_server_side_move_config(|config| config.is_enabled()) {
            false
        } else {
            self.sql
                .get_config_int(self, "mvbox_move")
                .map(|value| value == 1)
                .unwrap_or(true)
        }
    }

    /// Helper function that allows us to access the mutex protected server_side_move_configuration
    /// without having to write too verbose code.
    pub fn with_server_side_move_config<T>(&self, cb: impl FnOnce(&ServerSideMove) -> T) -> T {
        let arc = self.server_side_move_config.clone();
        let ssm = arc.lock().unwrap();
        cb(&ssm)
    }
}
