use crate::context::*;
use crate::imap::*;

impl Context {
    pub fn get_webpush_config(&self) -> Option<WebPushConfig> {
        self.inbox.read().unwrap().get_webpush_config()
    }
}
