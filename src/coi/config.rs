use crate::coi::CoiMessageFilter;

#[derive(Clone)]
pub struct CoiConfig {
    pub enabled: bool,
    pub mailbox_root: String,
    pub message_filter: CoiMessageFilter,
}

impl Default for CoiConfig {
    fn default() -> Self {
        CoiConfig {
            enabled: false,
            mailbox_root: "COI".into(),
            message_filter: CoiMessageFilter::default(),
        }
    }
}
