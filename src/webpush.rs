use crate::context::*;
use crate::imap::*;

pub struct WebPushConfig {
    vapid_key: String,
    subscription: Option<String>,
}

impl Context {
    pub fn is_webpush_supported(&self) -> bool {
        self.inbox.read().unwrap().is_webpush_supported()
    }
    pub fn get_webpush_config(&self) -> Option<WebPushConfig> {
        if !self.inbox.read().unwrap().is_webpush_supported() { return None; }
        let metadata = self.inbox.write().unwrap().get_metadata(self, "",
            &["/private/vendor/vendor.dovecot/webpush/subscription"],
            MetadataDepth::Infinity, None);
        Some(WebPushConfig {
            vapid_key: "".into(),
            subscription: None,
        })
    }
}
