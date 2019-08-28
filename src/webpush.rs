use crate::context::*;

#[derive(Clone)]
pub struct WebPushConfig {
    pub vapid: Option<String>,
}

impl Default for WebPushConfig {
    fn default() -> Self {
        WebPushConfig { vapid: None }
    }
}

impl Context {
    pub fn get_webpush_config(&self) -> Option<WebPushConfig> {
        self.inbox.read().unwrap().get_webpush_config()
    }
}
