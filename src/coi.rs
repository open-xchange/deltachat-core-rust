use crate::context::*;

impl Context {
    pub fn is_coi_supported(&self) -> bool {
        self.inbox.read().unwrap().is_coi_supported()
    }
}
