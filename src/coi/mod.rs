pub mod config;
pub mod message_filter;

pub use message_filter::CoiMessageFilter;
pub use config::CoiConfig;
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
        params.set_map(Param::Metadata,
                       &[(COI_METADATA_MESSAGE_FILTER, &message_filter.to_string())]);
        job_add(self, Action::SetMetadata, id as libc::c_int, params, 0);
    }

    pub fn get_coi_message_filter(&self, id: i32) {
        let mut params = Params::new();
        params.set(Param::Metadata, COI_METADATA_MESSAGE_FILTER);
        job_add(self, Action::GetMetadata, id as libc::c_int, params, 0);
    }
}
