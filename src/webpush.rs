use crate::context::*;
use crate::error::Result;
use crate::imap::*;

#[derive(Clone)]
pub struct WebPushConfig {
    pub vapid: Option<String>,
}

impl Default for WebPushConfig {
    fn default() -> Self {
        WebPushConfig { vapid: None }
    }
}

const SUBSCRIPTIONS: &str = "/private/vendor/vendor.dovecot/webpush/subscriptions/";

impl Context {
    pub fn get_webpush_config(&self) -> Option<WebPushConfig> {
        self.inbox.read().unwrap().get_webpush_config()
    }
    pub fn subscribe_webpush(&self, uid: &str, json: Option<&str>) -> Result<()> {
        self.inbox.read().unwrap().set_metadata(
            self,
            "",
            &[Metadata {
                entry: [SUBSCRIPTIONS, uid].concat(),
                value: json.map(|s| s.into()),
            }],
        )
    }
    pub fn get_webpush_subscription(&self, uid: &str) -> Result<Option<String>> {
        let res = self.inbox.read().unwrap().get_metadata(
            self,
            "",
            &[&[SUBSCRIPTIONS, uid].concat()],
            MetadataDepth::Zero,
            None,
        );
        Ok(res?.first().and_then(|m| m.value.clone()))
    }
    pub fn list_webpush_subscriptions(&self) -> Result<Vec<Metadata>> {
        self.inbox.read().unwrap().get_metadata(
            self,
            "",
            &[&SUBSCRIPTIONS[..SUBSCRIPTIONS.len() - 1]],
            MetadataDepth::One,
            None,
        )
    }
}
