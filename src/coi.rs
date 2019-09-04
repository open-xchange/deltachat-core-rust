use crate::coi_message_filter::CoiMessageFilter;
use crate::context::*;
use crate::error::Error;
use crate::imap::*;

impl Context {
    pub fn get_coi_config(&self) -> Option<CoiConfig> {
        self.inbox.read().unwrap().get_coi_config()
    }

    pub fn set_coi_enabled(&self, enable: bool) -> Result<(), Error> {
        self.inbox.write().unwrap().set_coi_enabled(self, enable)
    }

    pub fn set_coi_message_filter(&self, message_filter: CoiMessageFilter) -> Result<(), Error> {
        self.inbox
            .write()
            .unwrap()
            .set_coi_message_filter(self, message_filter)
    }

    pub fn get_coi_message_filter(&self) -> Result<CoiMessageFilter, Error> {
        self.inbox
            .write()
            .unwrap()
            .get_coi_message_filter(self)
    }
}
