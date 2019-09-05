use crate::coi::{CoiMessageFilter, ServerSideMove};

#[derive(Debug, Clone)]
pub struct CoiConfig {
    pub enabled: bool,
    pub mailbox_root: String,
    pub message_filter: CoiMessageFilter,
    pub imap_delimiter: char,
}

impl Default for CoiConfig {
    fn default() -> Self {
        CoiConfig {
            enabled: false,
            mailbox_root: "COI".into(),
            message_filter: CoiMessageFilter::default(),
            imap_delimiter: '.',
        }
    }
}

impl CoiConfig {
    pub fn get_server_side_move_config(&self) -> ServerSideMove {
        if self.server_side_performs_move() {
            ServerSideMove::Enabled {
                mvbox_folder_override: self.get_coi_chats_folder(),
            }
        } else {
            ServerSideMove::Disabled
        }
    }
    fn get_coi_chats_folder(&self) -> String {
        format!("{}{}Chats", self.mailbox_root, self.imap_delimiter)
    }

    /// Returns true if the IMAP server is configured to move COI messages automatically.
    fn server_side_performs_move(&self) -> bool {
        self.enabled
            && (self.message_filter == CoiMessageFilter::Active
                || self.message_filter == CoiMessageFilter::Seen)
    }
}

#[test]
fn it_should_return_correct_coi_chats_folder() {
    assert_eq!(
        "COI.Chats".to_string(),
        CoiConfig::default().get_coi_chats_folder()
    );
    assert_eq!(
        "ROOT/Chats".to_string(),
        CoiConfig {
            enabled: false,
            mailbox_root: "ROOT".into(),
            message_filter: CoiMessageFilter::default(),
            imap_delimiter: '/'
        }
        .get_coi_chats_folder()
    );
}
