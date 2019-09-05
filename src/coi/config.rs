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
                CoiMessageFilter::None => CoiDeltachatMode::coi_disabled(),
                CoiMessageFilter::Seen => CoiDeltachatMode {
                    server_side_move_enabled: true,
                    inbox_folder_override: Some("INBOX".to_string()),
                    mvbox_folder_override: Some(self.coi_chats_folder.to_string()),
                },
                CoiMessageFilter::Active => CoiDeltachatMode {
                    server_side_move_enabled: true,
                    inbox_folder_override: Some(self.coi_chats_folder.to_string()),
                    mvbox_folder_override: Some("INBOX".to_string()),
                },
            }
        } else {
            CoiDeltachatMode::coi_disabled()
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
