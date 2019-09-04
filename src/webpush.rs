use crate::context::*;
use crate::job::*;
use crate::param::*;

#[derive(Clone)]
pub struct WebPushConfig {
    pub vapid: Option<String>,
}

impl Default for WebPushConfig {
    fn default() -> Self {
        WebPushConfig { vapid: None }
    }
}

pub const SUBSCRIPTIONS: &str = "/private/vendor/vendor.dovecot/webpush/subscriptions/";

impl Context {
    pub fn get_webpush_config(&self) -> Option<WebPushConfig> {
        self.inbox.read().unwrap().get_webpush_config()
    }

    pub fn subscribe_webpush(&self, uid: &str, json: Option<&str>, id: i32) {
        let mut params = Params::new();
        params.set_map(Param::Metadata,
                       &[(&[SUBSCRIPTIONS, uid].concat(),
                          json.map_or("", |s| s.into()))]);
        job_add(self, Action::SetMetadata, id as libc::c_int, params, 0);
    }

    pub fn get_webpush_subscription(&self, uid: &str, id: i32) {
        let mut params = Params::new();
        params.set(Param::Metadata, uid);
        job_add(self, Action::GetWebPushSubscription, id as libc::c_int, params, 0);
    }

    pub fn validate_webpush(&self, uid: &str, msg: &str, id: i32) {
        let mut params = Params::new();
        params.set_map(Param::Metadata,
                       &[(&[SUBSCRIPTIONS, uid, "/validate"].concat(), msg)]);
        job_add(self, Action::SetMetadata, id as libc::c_int, params, 0);
    }
}
