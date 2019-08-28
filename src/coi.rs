use crate::coi_message_filter::CoiMessageFilter;
use crate::context::*;
use crate::error::Error;
use crate::imap::*;

impl Context {
    pub fn get_coi_config(&self) -> Option<CoiConfig> {
        self.inbox.read().unwrap().get_coi_config()
    }

    pub fn enable_coi(&self) -> Result<(), Error> {
        self.inbox.write().unwrap().enable_coi(self)
    }

    pub fn disable_coi(&self) -> Result<(), Error> {
        self.inbox.write().unwrap().disable_coi(self)
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
