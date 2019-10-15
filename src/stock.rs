use std::borrow::Cow;

use strum::EnumProperty;
use strum_macros::EnumProperty;

use crate::contact::*;
use crate::context::Context;
use crate::dc_tools::*;
use crate::events::Event;

/// Stock strings
///
/// These identify the string to return in [Context.stock_str].  The
/// numbers must stay in sync with `deltachat.h` `DC_STR_*` constants.
///
/// See the `stock_*` methods on [Context] to use these.
///
/// [Context]: crate::context::Context
#[derive(Debug, Clone, Copy, PartialEq, Eq, FromPrimitive, ToPrimitive, EnumProperty)]
#[repr(u32)]
pub enum StockMessage {
    #[strum(props(fallback = "No messages."))]
    NoMessages = 1,
    #[strum(props(fallback = "Me"))]
    SelfMsg = 2,
    #[strum(props(fallback = "Draft"))]
    Draft = 3,
    #[strum(props(fallback = "%1$s member(s)"))]
    Member = 4,
    #[strum(props(fallback = "%1$s contact(s)"))]
    Contact = 6,
    #[strum(props(fallback = "Voice message"))]
    VoiceMessage = 7,
    #[strum(props(fallback = "Contact requests"))]
    DeadDrop = 8,
    #[strum(props(fallback = "Image"))]
    Image = 9,
    #[strum(props(fallback = "Video"))]
    Video = 10,
    #[strum(props(fallback = "Audio"))]
    Audio = 11,
    #[strum(props(fallback = "File"))]
    File = 12,
    #[strum(props(fallback = "Sent with my Delta Chat Messenger: https://delta.chat"))]
    StatusLine = 13,
    #[strum(props(fallback = "Hello, I\'ve just created the group \"%1$s\" for us."))]
    NewGroupDraft = 14,
    #[strum(props(fallback = "Group name changed from \"%1$s\" to \"%2$s\"."))]
    MsgGrpName = 15,
    #[strum(props(fallback = "Group image changed."))]
    MsgGrpImgChanged = 16,
    #[strum(props(fallback = "Member %1$s added."))]
    MsgAddMember = 17,
    #[strum(props(fallback = "Member %1$s removed."))]
    MsgDelMember = 18,
    #[strum(props(fallback = "Group left."))]
    MsgGroupLeft = 19,
    #[strum(props(fallback = "GIF"))]
    Gif = 23,
    #[strum(props(fallback = "Encrypted message"))]
    EncryptedMsg = 24,
    #[strum(props(fallback = "End-to-end encryption available."))]
    E2eAvailable = 25,
    #[strum(props(fallback = "Transport-encryption."))]
    EncrTransp = 27,
    #[strum(props(fallback = "No encryption."))]
    EncrNone = 28,
    #[strum(props(fallback = "This message was encrypted for another setup."))]
    CantDecryptMsgBody = 29,
    #[strum(props(fallback = "Fingerprints"))]
    FingerPrints = 30,
    #[strum(props(fallback = "Return receipt"))]
    ReadRcpt = 31,
    #[strum(props(fallback = "This is a return receipt for the message \"%1$s\"."))]
    ReadRcptMailBody = 32,
    #[strum(props(fallback = "Group image deleted."))]
    MsgGrpImgDeleted = 33,
    #[strum(props(fallback = "End-to-end encryption preferred."))]
    E2ePreferred = 34,
    #[strum(props(fallback = "%1$s verified."))]
    ContactVerified = 35,
    #[strum(props(fallback = "Cannot verify %1$s"))]
    ContactNotVerified = 36,
    #[strum(props(fallback = "Changed setup for %1$s"))]
    ContactSetupChanged = 37,
    #[strum(props(fallback = "Archived chats"))]
    ArchivedChats = 40,
    #[strum(props(fallback = "Starred messages"))]
    StarredMsgs = 41,
    #[strum(props(fallback = "Autocrypt Setup Message"))]
    AcSetupMsgSubject = 42,
    #[strum(props(
        fallback = "This is the Autocrypt Setup Message used to transfer your key between clients.\n\nTo decrypt and use your key, open the message in an Autocrypt-compliant client and enter the setup code presented on the generating device."
    ))]
    AcSetupMsgBody = 43,
    #[strum(props(fallback = "Messages I sent to myself"))]
    SelfTalkSubTitle = 50,
    #[strum(props(fallback = "Cannot login as %1$s."))]
    CannotLogin = 60,
    #[strum(props(fallback = "Response from %1$s: %2$s"))]
    ServerResponse = 61,
    #[strum(props(fallback = "%1$s by %2$s."))]
    MsgActionByUser = 62,
    #[strum(props(fallback = "%1$s by me."))]
    MsgActionByMe = 63,
    #[strum(props(fallback = "Location streaming enabled."))]
    MsgLocationEnabled = 64,
    #[strum(props(fallback = "Location streaming disabled."))]
    MsgLocationDisabled = 65,
    #[strum(props(fallback = "Location"))]
    Location = 66,
    #[strum(props(fallback = "Sticker"))]
    Sticker = 67,
}

impl StockMessage {
    /// Default untranslated strings for stock messages.
    ///
    /// These could be used in logging calls, so no logging here.
    fn fallback(&self) -> &'static str {
        self.get_str("fallback").unwrap_or_default()
    }
}

impl Context {
    /// Return the stock string for the [StockMessage].
    ///
    /// If the context callback responds with a string to use, e.g. a
    /// translation, then this string will be returned.  Otherwise a
    /// default (English) string is returned.
    pub fn stock_str(&self, id: StockMessage) -> Cow<str> {
        let ptr = self.call_cb(Event::GetString { id, count: 0 }) as *mut libc::c_char;
        if ptr.is_null() {
            Cow::Borrowed(id.fallback())
        } else {
            let ret = to_string_lossy(ptr);
            unsafe { libc::free(ptr as *mut libc::c_void) };
            Cow::Owned(ret)
        }
    }

    /// Return stock string, replacing placeholders with provided string.
    ///
    /// This replaces both the *first* `%1$s`, `%1$d` and `%1$@`
    /// placeholders with the provided string.
    /// (the `%1$@` variant is used on iOS, the other are used on Android and Desktop)
    pub fn stock_string_repl_str(&self, id: StockMessage, insert: impl AsRef<str>) -> String {
        self.stock_str(id)
            .replacen("%1$s", insert.as_ref(), 1)
            .replacen("%1$d", insert.as_ref(), 1)
            .replacen("%1$@", insert.as_ref(), 1)
    }

    /// Return stock string, replacing placeholders with provided int.
    ///
    /// Like [Context::stock_string_repl_str] but substitute the placeholders
    /// with an integer.
    pub fn stock_string_repl_int(&self, id: StockMessage, insert: i32) -> String {
        self.stock_string_repl_str(id, format!("{}", insert).as_str())
    }

    /// Return stock string, replacing 2 placeholders with provided string.
    ///
    /// This replaces both the *first* `%1$s`, `%1$d` and `%1$@`
    /// placeholders with the string in `insert` and does the same for
    /// `%2$s`, `%2$d` and `%2$@` for `insert2`.
    /// (the `%1$@` variant is used on iOS, the other are used on Android and Desktop)
    fn stock_string_repl_str2(
        &self,
        id: StockMessage,
        insert: impl AsRef<str>,
        insert2: impl AsRef<str>,
    ) -> String {
        self.stock_str(id)
            .replacen("%1$s", insert.as_ref(), 1)
            .replacen("%1$d", insert.as_ref(), 1)
            .replacen("%1$@", insert.as_ref(), 1)
            .replacen("%2$s", insert2.as_ref(), 1)
            .replacen("%2$d", insert2.as_ref(), 1)
            .replacen("%2$@", insert2.as_ref(), 1)
    }

    /// Return some kind of stock message
    ///
    /// If the `id` is [StockMessage::MsgAddMember] or
    /// [StockMessage::MsgDelMember] then `param1` is considered to be the
    /// contact address and will be replaced by that contact's display
    /// name.
    ///
    /// If `from_id` is not `0`, any trailing dot is removed from the
    /// first stock string created so far.  If the `from_id` contact is
    /// the user itself, i.e. `DC_CONTACT_ID_SELF` the string is used
    /// itself as param to the [StockMessage::MsgActionByMe] stock string
    /// resulting in a string like "Member Alice added by me." (for
    /// [StockMessage::MsgAddMember] as `id`).  If the `from_id` contact
    /// is any other user than the contact's display name is looked up and
    /// used as the second parameter to [StockMessage::MsgActionByUser] with
    /// again the original stock string being used as the first parameter,
    /// resulting in a string like "Member Alice added by Bob.".
    pub fn stock_system_msg(
        &self,
        id: StockMessage,
        param1: impl AsRef<str>,
        param2: impl AsRef<str>,
        from_id: u32,
    ) -> String {
        let insert1 = if id == StockMessage::MsgAddMember || id == StockMessage::MsgDelMember {
            let contact_id = Contact::lookup_id_by_addr(self, param1.as_ref());
            if contact_id != 0 {
                Contact::get_by_id(self, contact_id)
                    .map(|contact| contact.get_name_n_addr())
                    .unwrap_or_default()
            } else {
                param1.as_ref().to_string()
            }
        } else {
            param1.as_ref().to_string()
        };

        let action = self.stock_string_repl_str2(id, insert1, param2.as_ref().to_string());
        let action1 = action.trim_end_matches('.');
        match from_id {
            0 => action,
            1 => self.stock_string_repl_str(StockMessage::MsgActionByMe, action1), // DC_CONTACT_ID_SELF
            _ => {
                let displayname = Contact::get_by_id(self, from_id)
                    .map(|contact| contact.get_name_n_addr())
                    .unwrap_or_default();

                self.stock_string_repl_str2(StockMessage::MsgActionByUser, action1, &displayname)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_utils::*;

    use crate::constants::DC_CONTACT_ID_SELF;
    use libc::uintptr_t;

    use num_traits::ToPrimitive;

    #[test]
    fn test_enum_mapping() {
        assert_eq!(StockMessage::NoMessages.to_usize().unwrap(), 1);
        assert_eq!(StockMessage::SelfMsg.to_usize().unwrap(), 2);
    }

    #[test]
    fn test_fallback() {
        assert_eq!(StockMessage::NoMessages.fallback(), "No messages.");
    }

    #[test]
    fn test_stock_str() {
        let t = dummy_context();
        assert_eq!(t.ctx.stock_str(StockMessage::NoMessages), "No messages.");
    }

    fn test_stock_str_no_fallback_cb(_ctx: &Context, evt: Event) -> uintptr_t {
        match evt {
            Event::GetString {
                id: StockMessage::NoMessages,
                ..
            } => unsafe { "Hello there".strdup() as usize },
            _ => 0,
        }
    }

    #[test]
    fn test_stock_str_no_fallback() {
        let t = test_context(Some(Box::new(test_stock_str_no_fallback_cb)));
        assert_eq!(t.ctx.stock_str(StockMessage::NoMessages), "Hello there");
    }

    #[test]
    fn test_stock_string_repl_str() {
        let t = dummy_context();
        // uses %1$s substitution
        assert_eq!(
            t.ctx.stock_string_repl_str(StockMessage::Member, "42"),
            "42 member(s)"
        );
        // We have no string using %1$d to test...
    }

    #[test]
    fn test_stock_string_repl_int() {
        let t = dummy_context();
        assert_eq!(
            t.ctx.stock_string_repl_int(StockMessage::Member, 42),
            "42 member(s)"
        );
    }

    #[test]
    fn test_stock_string_repl_str2() {
        let t = dummy_context();
        assert_eq!(
            t.ctx
                .stock_string_repl_str2(StockMessage::ServerResponse, "foo", "bar"),
            "Response from foo: bar"
        );
    }

    #[test]
    fn test_stock_system_msg_simple() {
        let t = dummy_context();
        assert_eq!(
            t.ctx
                .stock_system_msg(StockMessage::MsgLocationEnabled, "", "", 0),
            "Location streaming enabled."
        )
    }

    #[test]
    fn test_stock_system_msg_add_member_by_me() {
        let t = dummy_context();
        assert_eq!(
            t.ctx.stock_system_msg(
                StockMessage::MsgAddMember,
                "alice@example.com",
                "",
                DC_CONTACT_ID_SELF
            ),
            "Member alice@example.com added by me."
        )
    }

    #[test]
    fn test_stock_system_msg_add_member_by_me_with_displayname() {
        let t = dummy_context();
        Contact::create(&t.ctx, "Alice", "alice@example.com").expect("failed to create contact");
        assert_eq!(
            t.ctx.stock_system_msg(
                StockMessage::MsgAddMember,
                "alice@example.com",
                "",
                DC_CONTACT_ID_SELF
            ),
            "Member Alice (alice@example.com) added by me."
        );
    }

    #[test]
    fn test_stock_system_msg_add_member_by_other_with_displayname() {
        let t = dummy_context();
        let contact_id = {
            Contact::create(&t.ctx, "Alice", "alice@example.com")
                .expect("Failed to create contact Alice");
            Contact::create(&t.ctx, "Bob", "bob@example.com").expect("failed to create bob")
        };
        assert_eq!(
            t.ctx.stock_system_msg(
                StockMessage::MsgAddMember,
                "alice@example.com",
                "",
                contact_id,
            ),
            "Member Alice (alice@example.com) added by Bob (bob@example.com)."
        );
    }

    #[test]
    fn test_stock_system_msg_grp_name() {
        let t = dummy_context();
        assert_eq!(
            t.ctx.stock_system_msg(
                StockMessage::MsgGrpName,
                "Some chat",
                "Other chat",
                DC_CONTACT_ID_SELF
            ),
            "Group name changed from \"Some chat\" to \"Other chat\" by me."
        )
    }

    #[test]
    fn test_stock_system_msg_grp_name_other() {
        let t = dummy_context();
        let id = Contact::create(&t.ctx, "Alice", "alice@example.com")
            .expect("failed to create contact");

        assert_eq!(
            t.ctx
                .stock_system_msg(StockMessage::MsgGrpName, "Some chat", "Other chat", id,),
            "Group name changed from \"Some chat\" to \"Other chat\" by Alice (alice@example.com)."
        )
    }
}
