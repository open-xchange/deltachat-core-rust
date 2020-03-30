use crate::coi::{CoiDeltachatMode, CoiMessageFilter};

#[derive(Debug, Clone)]
pub struct CoiConfig {
    pub enabled: bool,
    coi_chats_folder: String,
    pub message_filter: CoiMessageFilter,
}

impl Default for CoiConfig {
    fn default() -> Self {
        CoiConfig {
            enabled: false,
            coi_chats_folder: "COI/Chats".into(),
            message_filter: CoiMessageFilter::default(),
        }
    }
}

impl CoiConfig {
    pub fn get_coi_deltachat_mode(&self) -> CoiDeltachatMode {
        if self.enabled {
            match self.message_filter {
                CoiMessageFilter::None => CoiDeltachatMode::Disabled,
                CoiMessageFilter::Seen => CoiDeltachatMode::Enabled {
                    inbox_folder_override: "INBOX".to_string(),
                    mvbox_folder_override: self.coi_chats_folder.to_string(),
                },
                CoiMessageFilter::Active => CoiDeltachatMode::Enabled {
                    inbox_folder_override: "INBOX".to_string(),
                    mvbox_folder_override: self.coi_chats_folder.to_string(),
                },
            }
        } else {
            CoiDeltachatMode::Disabled
        }
    }

    pub fn set_mailbox_root(&mut self, new_mailbox_root: &str) {
        self.coi_chats_folder = format!("{}/Chats", new_mailbox_root);
    }
}

#[test]
fn it_should_return_correct_coi_chats_folder() {
    let mut config = CoiConfig::default();

    assert_eq!("COI/Chats", config.coi_chats_folder);

    config.set_mailbox_root("ROOT");

    assert_eq!("ROOT/Chats", config.coi_chats_folder);
}
