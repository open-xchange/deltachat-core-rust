use crate::coi::CoiMessageFilter;

#[derive(Debug, Clone)]
pub struct CoiConfig {
    pub enabled: bool,
    pub coi_root: String,
    pub message_filter: CoiMessageFilter,
}

impl Default for CoiConfig {
    fn default() -> Self {
        CoiConfig {
            enabled: false,
            coi_root: "COI".to_string(),
            message_filter: CoiMessageFilter::default(),
        }
    }
}
