use crate::context::*;
use crate::imap::*;

impl Context {
    pub fn get_coi_config(&self) -> Option<CoiConfig> {
        self.inbox.read().unwrap().get_coi_config()
    }
}
