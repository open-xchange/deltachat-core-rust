use itertools::Itertools;
use num_traits::{FromPrimitive, ToPrimitive};
use rusqlite;
use rusqlite::types::*;

use crate::aheader::EncryptPreference;
use crate::config::Config;
use crate::constants::*;
use crate::context::Context;
use crate::dc_e2ee::*;
use crate::dc_loginparam::*;
use crate::dc_tools::*;
use crate::error::Result;
use crate::key::*;
use crate::message::MessageState;
use crate::peerstate::*;
use crate::sql;
use crate::stock::StockMessage;
use crate::types::*;

const DC_GCL_VERIFIED_ONLY: u32 = 0x01;

/// Contacts with at least this origin value are shown in the contact list.
const DC_ORIGIN_MIN_CONTACT_LIST: i32 = 0x100;

/// An object representing a single contact in memory.
/// The contact object is not updated.
/// If you want an update, you have to recreate the object.
///
/// The library makes sure
/// only to use names _authorized_ by the contact in `To:` or `Cc:`.
/// _Given-names _as "Daddy" or "Honey" are not used there.
/// For this purpose, internally, two names are tracked -
/// authorized-name and given-name.
/// By default, these names are equal, but functions working with contact names
pub struct Contact<'a> {
    context: &'a Context,
    /// The contact ID.
    ///
    /// Special message IDs:
    /// - DC_CONTACT_ID_SELF (1) - this is the owner of the account with the email-address set by
    ///   `dc_set_config` using "addr".
    ///
    /// Normal contact IDs are larger than these special ones (larger than DC_CONTACT_ID_LAST_SPECIAL).
    pub id: u32,
    /// Contact name. It is recommended to use `Contact::get_name`,
    /// `Contact::get_display_name` or `Contact::get_name_n_addr` to access this field.
    /// May be empty, initially set to `authname`.
    name: String,
    /// Name authorized by the contact himself. Only this name may be spread to others,
    /// e.g. in To:-lists. May be empty. It is recommended to use `Contact::get_name`,
    /// `Contact::get_display_name` or `Contact::get_name_n_addr` to access this field.
    authname: String,
    /// E-Mail-Address of the contact. It is recommended to use `Contact::get_addr`` to access this field.
    addr: String,
    /// Blocked state. Use dc_contact_is_blocked to access this field.
    blocked: bool,
    /// The origin/source of the contact.
    pub origin: Origin,
}

/// Possible origins of a contact.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, FromPrimitive, ToPrimitive)]
#[repr(i32)]
pub enum Origin {
    Unknown = 0,
    /// From: of incoming messages of unknown sender
    IncomingUnknownFrom = 0x10,
    /// Cc: of incoming messages of unknown sender
    IncomingUnknownCc = 0x20,
    /// To: of incoming messages of unknown sender
    IncomingUnknownTo = 0x40,
    /// address scanned but not verified
    UnhandledQrScan = 0x80,
    /// Reply-To: of incoming message of known sender
    IncomingReplyTo = 0x100,
    /// Cc: of incoming message of known sender
    IncomingCc = 0x200,
    /// additional To:'s of incoming message of known sender
    IncomingTo = 0x400,
    /// a chat was manually created for this user, but no message yet sent
    CreateChat = 0x800,
    /// message sent by us
    OutgoingBcc = 0x1000,
    /// message sent by us
    OutgoingCc = 0x2000,
    /// message sent by us
    OutgoingTo = 0x4000,
    /// internal use
    Internal = 0x40000,
    /// address is in our address book
    AdressBook = 0x80000,
    /// set on Alice's side for contacts like Bob that have scanned the QR code offered by her. Only means the contact has once been established using the "securejoin" procedure in the past, getting the current key verification status requires calling dc_contact_is_verified() !
    SecurejoinInvited = 0x1000000,
    /// set on Bob's side for contacts scanned and verified from a QR code. Only means the contact has once been established using the "securejoin" procedure in the past, getting the current key verification status requires calling dc_contact_is_verified() !
    SecurejoinJoined = 0x2000000,
    /// contact added mannually by dc_create_contact(), this should be the largets origin as otherwise the user cannot modify the names
    ManuallyCreated = 0x4000000,
}

impl ToSql for Origin {
    fn to_sql(&self) -> rusqlite::Result<ToSqlOutput> {
        let num: i64 = self
            .to_i64()
            .expect("impossible: Origin -> i64 conversion failed");

        Ok(ToSqlOutput::Owned(Value::Integer(num)))
    }
}

impl FromSql for Origin {
    fn column_result(col: ValueRef) -> FromSqlResult<Self> {
        let inner = FromSql::column_result(col)?;
        FromPrimitive::from_i64(inner).ok_or(FromSqlError::InvalidType)
    }
}

impl Origin {
    /// Contacts that start a new "normal" chat, defaults to off.
    pub fn is_start_new_chat(self) -> bool {
        self as i32 >= 0x7FFFFFFF
    }

    /// Contacts that are verified and known not to be spam.
    pub fn is_verified(self) -> bool {
        self as i32 >= 0x100
    }

    /// Contacts that are shown in the contact list.
    pub fn include_in_contactlist(self) -> bool {
        self as i32 >= DC_ORIGIN_MIN_CONTACT_LIST
    }
}

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum Modifier {
    None,
    Modified,
    Created,
}

#[derive(Debug, PartialEq, Eq, Clone, Copy, FromPrimitive)]
#[repr(u8)]
pub enum VerifiedStatus {
    /// Contact is not verified.
    Unverified = 0,
    // TODO: is this a thing?
    Verified = 1,
    /// SELF and contact have verified their fingerprints in both directions; in the UI typically checkmarks are shown.
    BidirectVerified = 2,
}

impl<'a> Contact<'a> {
    pub fn load_from_db(context: &'a Context, contact_id: u32) -> Result<Self> {
        if contact_id == DC_CONTACT_ID_SELF as u32 {
            let contact = Contact {
                context,
                id: contact_id,
                name: context.stock_str(StockMessage::SelfMsg).into(),
                authname: "".into(),
                addr: context
                    .get_config(Config::ConfiguredAddr)
                    .unwrap_or_default(),
                blocked: false,
                origin: Origin::Unknown,
            };

            return Ok(contact);
        }

        context.sql.query_row(
            "SELECT c.name, c.addr, c.origin, c.blocked, c.authname  FROM contacts c  WHERE c.id=?;",
            params![contact_id as i32],
            |row| {
                let contact = Self {
                    context,
                    id: contact_id,
                    name: row.get::<_, String>(0)?,
                    authname: row.get::<_, String>(4)?,
                    addr: row.get::<_, String>(1)?,
                    blocked: row.get::<_, Option<i32>>(3)?.unwrap_or_default() != 0,
                    origin: row.get(2)?,
                };
                Ok(contact)
            }
        )
    }

    /// Returns `true` if this contact is blocked.
    pub fn is_blocked(&self) -> bool {
        self.blocked
    }

    /// Check if a contact is blocked.
    pub fn is_blocked_load(context: &'a Context, id: u32) -> bool {
        Self::load_from_db(context, id)
            .map(|contact| contact.blocked)
            .unwrap_or_default()
    }

    /// Block the given contact.
    pub fn block(context: &Context, id: u32) {
        set_block_contact(context, id, true);
    }

    /// Unblock the given contact.
    pub fn unblock(context: &Context, id: u32) {
        set_block_contact(context, id, false);
    }

    /// Add a single contact as a result of an _explicit_ user action.
    ///
    /// We assume, the contact name, if any, is entered by the user and is used "as is" therefore,
    /// normalize() is _not_ called for the name. If the contact is blocked, it is unblocked.
    ///
    /// To add a number of contacts, see `dc_add_address_book()` which is much faster for adding
    /// a bunch of addresses.
    ///
    /// May result in a `#DC_EVENT_CONTACTS_CHANGED` event.
    pub fn create(context: &Context, name: impl AsRef<str>, addr: impl AsRef<str>) -> Result<u32> {
        ensure!(
            !addr.as_ref().is_empty(),
            "Cannot create contact with empty address"
        );

        let (contact_id, sth_modified) =
            Contact::add_or_lookup(context, name, addr, Origin::ManuallyCreated)?;
        let blocked = Contact::is_blocked_load(context, contact_id);
        context.call_cb(
            Event::CONTACTS_CHANGED,
            (if sth_modified == Modifier::Created {
                contact_id
            } else {
                0
            }) as uintptr_t,
            0 as uintptr_t,
        );
        if blocked {
            Contact::unblock(context, contact_id);
        }

        Ok(contact_id)
    }

    /// Mark all messages sent by the given contact
    /// as _noticed_.  See also dc_marknoticed_chat() and dc_markseen_msgs()
    ///
    /// Calling this function usually results in the event `#DC_EVENT_MSGS_CHANGED`.
    pub fn mark_noticed(context: &Context, id: u32) {
        if sql::execute(
            context,
            &context.sql,
            "UPDATE msgs SET state=? WHERE from_id=? AND state=?;",
            params![MessageState::InNoticed, id as i32, MessageState::InFresh],
        )
        .is_ok()
        {
            context.call_cb(Event::MSGS_CHANGED, 0, 0);
        }
    }

    /// Check if an e-mail address belongs to a known and unblocked contact.
    /// Known and unblocked contacts will be returned by `dc_get_contacts()`.
    ///
    /// To validate an e-mail address independently of the contact database
    /// use `dc_may_be_valid_addr()`.
    pub fn lookup_id_by_addr(context: &Context, addr: impl AsRef<str>) -> u32 {
        if addr.as_ref().is_empty() {
            return 0;
        }

        let addr_normalized = addr_normalize(addr.as_ref());
        let addr_self = context
            .get_config(Config::ConfiguredAddr)
            .unwrap_or_default();

        if addr_normalized == addr_self {
            return 1;
        }

        context.sql.query_row_col(
            context,
            "SELECT id FROM contacts WHERE addr=?1 COLLATE NOCASE AND id>?2 AND origin>=?3 AND blocked=0;",
            params![
                addr_normalized,
                DC_CONTACT_ID_LAST_SPECIAL as i32,
                DC_ORIGIN_MIN_CONTACT_LIST,
            ],
            0
        ).unwrap_or_default()
    }

    /// Lookup a contact and create it if it does not exist yet.
    ///
    /// Returns the contact_id and a `Modifier` value indicating if a modification occured.
    pub fn add_or_lookup(
        context: &Context,
        name: impl AsRef<str>,
        addr: impl AsRef<str>,
        origin: Origin,
    ) -> Result<(u32, Modifier)> {
        let mut sth_modified = Modifier::None;

        ensure!(
            !addr.as_ref().is_empty(),
            "Can not add_or_lookup empty address"
        );
        ensure!(origin != Origin::Unknown, "Missing valid origin");

        let addr = addr_normalize(addr.as_ref());
        let addr_self = context
            .get_config(Config::ConfiguredAddr)
            .unwrap_or_default();

        if addr == addr_self {
            return Ok((1, sth_modified));
        }

        if !may_be_valid_addr(&addr) {
            warn!(
                context,
                0,
                "Bad address \"{}\" for contact \"{}\".",
                addr,
                if !name.as_ref().is_empty() {
                    name.as_ref()
                } else {
                    "<unset>"
                },
            );
            bail!("Bad address supplied: {:?}", addr);
        }

        let mut update_addr = false;
        let mut update_name = false;
        let mut update_authname = false;
        let mut row_id = 0;

        if let Ok((id, row_name, row_addr, row_origin, row_authname)) = context.sql.query_row(
            "SELECT id, name, addr, origin, authname FROM contacts WHERE addr=? COLLATE NOCASE;",
            params![addr],
            |row| {
                let row_id = row.get(0)?;
                let row_name: String = row.get(1)?;
                let row_addr: String = row.get(2)?;
                let row_origin = row.get(3)?;
                let row_authname: String = row.get(4)?;

                if !name.as_ref().is_empty() && !row_name.is_empty() {
                    if origin >= row_origin && name.as_ref() != row_name {
                        update_name = true;
                    }
                } else {
                    update_name = true;
                }
                if origin == Origin::IncomingUnknownFrom && name.as_ref() != row_authname {
                    update_authname = true;
                }
                Ok((row_id, row_name, row_addr, row_origin, row_authname))
            },
        ) {
            row_id = id;
            if origin as i32 >= row_origin as i32 && addr != row_addr {
                update_addr = true;
            }
            if update_name || update_authname || update_addr || origin > row_origin {
                sql::execute(
                    context,
                    &context.sql,
                    "UPDATE contacts SET name=?, addr=?, origin=?, authname=? WHERE id=?;",
                    params![
                        if update_name {
                            name.as_ref()
                        } else {
                            &row_name
                        },
                        if update_addr { addr } else { &row_addr },
                        if origin > row_origin {
                            origin
                        } else {
                            row_origin
                        },
                        if update_authname {
                            name.as_ref()
                        } else {
                            &row_authname
                        },
                        row_id
                    ],
                )
                .ok();

                if update_name {
                    sql::execute(
                    context,
                    &context.sql,
                    "UPDATE chats SET name=? WHERE type=? AND id IN(SELECT chat_id FROM chats_contacts WHERE contact_id=?);",
                    params![name.as_ref(), 100, row_id]
                ).ok();
                }
                sth_modified = Modifier::Modified;
            }
        } else {
            if sql::execute(
                context,
                &context.sql,
                "INSERT INTO contacts (name, addr, origin) VALUES(?, ?, ?);",
                params![name.as_ref(), addr, origin,],
            )
            .is_ok()
            {
                row_id = sql::get_rowid(context, &context.sql, "contacts", "addr", addr);
                sth_modified = Modifier::Created;
            } else {
                error!(context, 0, "Cannot add contact.");
            }
        }

        Ok((row_id, sth_modified))
    }

    /// Add a number of contacts.
    ///
    /// Typically used to add the whole address book from the OS. As names here are typically not
    /// well formatted, we call `normalize()` for each name given.
    ///
    /// No email-address is added twice.
    /// Trying to add email-addresses that are already in the contact list,
    /// results in updating the name unless the name was changed manually by the user.
    /// If any email-address or any name is really updated,
    /// the event `DC_EVENT_CONTACTS_CHANGED` is sent.
    ///
    /// To add a single contact entered by the user, you should prefer `Contact::create`,
    /// however, for adding a bunch of addresses, this function is _much_ faster.
    ///
    /// The `adr_book` is a multiline string in the format `Name one\nAddress one\nName two\nAddress two`.
    ///
    /// Returns the number of modified contacts.
    pub fn add_address_book(context: &Context, adr_book: impl AsRef<str>) -> Result<usize> {
        let mut modify_cnt = 0;

        for chunk in &adr_book.as_ref().lines().chunks(2) {
            let chunk = chunk.collect::<Vec<_>>();
            if chunk.len() < 2 {
                break;
            }
            let name = chunk[0];
            let addr = chunk[1];
            let name = normalize_name(name);
            let (_, modified) = Contact::add_or_lookup(context, name, addr, Origin::AdressBook)?;
            if modified != Modifier::None {
                modify_cnt += 1
            }
        }
        if modify_cnt > 0 {
            context.call_cb(Event::CONTACTS_CHANGED, 0 as uintptr_t, 0 as uintptr_t);
        }

        Ok(modify_cnt)
    }

    /// Returns known and unblocked contacts.
    ///
    /// To get information about a single contact, see dc_get_contact().
    ///
    /// `listflags` is a combination of flags:
    /// - if the flag DC_GCL_ADD_SELF is set, SELF is added to the list unless filtered by other parameters
    /// - if the flag DC_GCL_VERIFIED_ONLY is set, only verified contacts are returned.
    ///   if DC_GCL_VERIFIED_ONLY is not set, verified and unverified contacts are returned.
    /// `query` is a string to filter the list.
    pub fn get_all(
        context: &Context,
        listflags: u32,
        query: Option<impl AsRef<str>>,
    ) -> Result<Vec<u32>> {
        let self_addr = context
            .get_config(Config::ConfiguredAddr)
            .unwrap_or_default();

        let mut add_self = false;
        let mut ret = Vec::new();

        if (listflags & DC_GCL_VERIFIED_ONLY) > 0 || query.is_some() {
            let s3str_like_cmd = format!(
                "%{}%",
                query
                    .as_ref()
                    .map(|s| s.as_ref().to_string())
                    .unwrap_or_default()
            );
            context.sql.query_map(
                "SELECT c.id FROM contacts c \
                 LEFT JOIN acpeerstates ps ON c.addr=ps.addr  \
                 WHERE c.addr!=?1 \
                 AND c.id>?2 \
                 AND c.origin>=?3 \
                 AND c.blocked=0 \
                 AND (c.name LIKE ?4 OR c.addr LIKE ?5) \
                 AND (1=?6 OR LENGTH(ps.verified_key_fingerprint)!=0)  \
                 ORDER BY LOWER(c.name||c.addr),c.id;",
                params![
                    self_addr,
                    DC_CONTACT_ID_LAST_SPECIAL as i32,
                    0x100,
                    &s3str_like_cmd,
                    &s3str_like_cmd,
                    if 0 != listflags & 0x1 { 0 } else { 1 },
                ],
                |row| row.get::<_, i32>(0),
                |ids| {
                    for id in ids {
                        ret.push(id? as u32);
                    }
                    Ok(())
                },
            )?;

            let self_name = context.get_config(Config::Displayname).unwrap_or_default();
            let self_name2 = context.stock_str(StockMessage::SelfMsg);

            if let Some(query) = query {
                if self_addr.contains(query.as_ref())
                    || self_name.contains(query.as_ref())
                    || self_name2.contains(query.as_ref())
                {
                    add_self = true;
                }
            } else {
                add_self = true;
            }
        } else {
            add_self = true;

            context.sql.query_map(
                "SELECT id FROM contacts WHERE addr!=?1 AND id>?2 AND origin>=?3 AND blocked=0 ORDER BY LOWER(name||addr),id;",
                params![self_addr, DC_CONTACT_ID_LAST_SPECIAL as i32, 0x100],
                |row| row.get::<_, i32>(0),
                |ids| {
                    for id in ids {
                        ret.push(id? as u32);
                    }
                    Ok(())
                }
            )?;
        }

        if 0 != listflags & DC_GCL_ADD_SELF as u32 && add_self {
            ret.push(DC_CONTACT_ID_SELF as u32);
        }

        Ok(ret)
    }

    pub fn get_blocked_cnt(context: &Context) -> usize {
        context
            .sql
            .query_row_col::<_, isize>(
                context,
                "SELECT COUNT(*) FROM contacts WHERE id>? AND blocked!=0",
                params![DC_CONTACT_ID_LAST_SPECIAL as i32],
                0,
            )
            .unwrap_or_default() as usize
    }

    /// Get blocked contacts.
    pub fn get_all_blocked(context: &Context) -> Vec<u32> {
        context
            .sql
            .query_map(
                "SELECT id FROM contacts WHERE id>? AND blocked!=0 ORDER BY LOWER(name||addr),id;",
                params![DC_CONTACT_ID_LAST_SPECIAL as i32],
                |row| row.get::<_, u32>(0),
                |ids| {
                    ids.collect::<std::result::Result<Vec<_>, _>>()
                        .map_err(Into::into)
                },
            )
            .unwrap_or_default()
    }

    /// Returns a textual summary of the encryption state for the contact.
    ///
    /// This function returns a string explaining the encryption state
    /// of the contact and if the connection is encrypted the
    /// fingerprints of the keys involved.
    pub fn get_encrinfo(context: &Context, contact_id: u32) -> Result<String> {
        let mut ret = String::new();

        if let Ok(contact) = Contact::load_from_db(context, contact_id) {
            let peerstate = Peerstate::from_addr(context, &context.sql, &contact.addr);
            let loginparam = dc_loginparam_read(context, &context.sql, "configured_");

            let mut self_key = Key::from_self_public(context, &loginparam.addr, &context.sql);

            if peerstate.is_some() && peerstate.as_ref().and_then(|p| p.peek_key(0)).is_some() {
                let peerstate = peerstate.as_ref().unwrap();
                let p =
                    context.stock_str(if peerstate.prefer_encrypt == EncryptPreference::Mutual {
                        StockMessage::E2ePreferred
                    } else {
                        StockMessage::E2eAvailable
                    });
                ret += &p;
                if self_key.is_none() {
                    dc_ensure_secret_key_exists(context)?;
                    self_key = Key::from_self_public(context, &loginparam.addr, &context.sql);
                }
                let p = context.stock_str(StockMessage::FingerPrints);
                ret += &format!(" {}:", p);

                let fingerprint_self = self_key
                    .map(|k| k.formatted_fingerprint())
                    .unwrap_or_default();
                let fingerprint_other_verified = peerstate
                    .peek_key(2)
                    .map(|k| k.formatted_fingerprint())
                    .unwrap_or_default();
                let fingerprint_other_unverified = peerstate
                    .peek_key(0)
                    .map(|k| k.formatted_fingerprint())
                    .unwrap_or_default();
                if peerstate.addr.is_some() && &loginparam.addr < peerstate.addr.as_ref().unwrap() {
                    cat_fingerprint(&mut ret, &loginparam.addr, &fingerprint_self, "");
                    cat_fingerprint(
                        &mut ret,
                        peerstate.addr.as_ref().unwrap(),
                        &fingerprint_other_verified,
                        &fingerprint_other_unverified,
                    );
                } else {
                    cat_fingerprint(
                        &mut ret,
                        peerstate.addr.as_ref().unwrap(),
                        &fingerprint_other_verified,
                        &fingerprint_other_unverified,
                    );
                    cat_fingerprint(&mut ret, &loginparam.addr, &fingerprint_self, "");
                }
            } else if 0 == loginparam.server_flags & DC_LP_IMAP_SOCKET_PLAIN as i32
                && 0 == loginparam.server_flags & DC_LP_SMTP_SOCKET_PLAIN as i32
            {
                ret += &context.stock_str(StockMessage::EncrTransp);
            } else {
                ret += &context.stock_str(StockMessage::EncrNone);
            }
        }

        Ok(ret)
    }

    /// Delete a contact. The contact is deleted from the local device. It may happen that this is not
    /// possible as the contact is in use. In this case, the contact can be blocked.
    ///
    /// May result in a `#DC_EVENT_CONTACTS_CHANGED` event.
    pub fn delete(context: &Context, contact_id: u32) -> Result<()> {
        ensure!(
            contact_id > DC_CONTACT_ID_LAST_SPECIAL as u32,
            "Can not delete special contact"
        );

        let count_contacts: i32 = context
            .sql
            .query_row_col(
                context,
                "SELECT COUNT(*) FROM chats_contacts WHERE contact_id=?;",
                params![contact_id as i32],
                0,
            )
            .unwrap_or_default();

        let count_msgs: i32 = if count_contacts > 0 {
            context
                .sql
                .query_row_col(
                    context,
                    "SELECT COUNT(*) FROM msgs WHERE from_id=? OR to_id=?;",
                    params![contact_id as i32, contact_id as i32],
                    0,
                )
                .unwrap_or_default()
        } else {
            0
        };

        if count_msgs == 0 {
            match sql::execute(
                context,
                &context.sql,
                "DELETE FROM contacts WHERE id=?;",
                params![contact_id as i32],
            ) {
                Ok(_) => {
                    context.call_cb(Event::CONTACTS_CHANGED, 0, 0);
                    return Ok(());
                }
                Err(err) => {
                    error!(context, 0, "delete_contact {} failed ({})", contact_id, err);
                    return Err(err);
                }
            }
        }

        info!(
            context,
            0, "could not delete contact {}, there are {} messages with it", contact_id, count_msgs
        );
        bail!("Could not delete contact with messages in it");
    }

    /// Get a single contact object.  For a list, see eg. dc_get_contacts().
    ///
    /// For contact DC_CONTACT_ID_SELF (1), the function returns sth.
    /// like "Me" in the selected language and the email address
    /// defined by dc_set_config().
    pub fn get_by_id(context: &Context, contact_id: u32) -> Result<Contact> {
        Contact::load_from_db(context, contact_id)
    }

    /// Get the ID of the contact.
    pub fn get_id(&self) -> u32 {
        self.id
    }

    /// Get email address. The email address is always set for a contact.
    pub fn get_addr(&self) -> &str {
        &self.addr
    }

    pub fn get_authname(&self) -> &str {
        &self.authname
    }

    /// Get the contact name. This is the name as defined by the contact himself or
    /// modified by the user.  May be an empty string.
    ///
    /// This name is typically used in a form where the user can edit the name of a contact.
    /// To get a fine name to display in lists etc., use `Contact::get_display_name` or `Contact::get_name_n_addr`.
    pub fn get_name(&self) -> &str {
        &self.name
    }

    /// Get display name. This is the name as defined by the contact himself,
    /// modified by the user or, if both are unset, the email address.
    ///
    /// This name is typically used in lists.
    /// To get the name editable in a formular, use `Contact::get_name`.
    pub fn get_display_name(&self) -> &str {
        if !self.name.is_empty() {
            return &self.name;
        }
        &self.addr
    }

    /// Get a summary of name and address.
    ///
    /// The returned string is either "Name (email@domain.com)" or just
    /// "email@domain.com" if the name is unset.
    ///
    /// The summary is typically used when asking the user something about the contact.
    /// The attached email address makes the question unique, eg. "Chat with Alan Miller (am@uniquedomain.com)?"
    pub fn get_name_n_addr(&self) -> String {
        if !self.name.is_empty() {
            return format!("{} ({})", self.name, self.addr);
        }
        (&self.addr).into()
    }

    /// Get the part of the name before the first space. In most languages, this seems to be
    /// the prename. If there is no space, the full display name is returned.
    /// If the display name is not set, the e-mail address is returned.
    pub fn get_first_name(&self) -> &str {
        if !self.name.is_empty() {
            return get_first_name(&self.name);
        }
        &self.addr
    }

    /// Get the contact's profile image.
    /// This is the image set by each remote user on their own
    /// using dc_set_config(context, "selfavatar", image).
    pub fn get_profile_image(&self) -> Option<String> {
        if self.id == DC_CONTACT_ID_SELF as u32 {
            return self.context.get_config(Config::Selfavatar);
        }
        // TODO: else get image_abs from contact param
        None
    }

    /// Get a color for the contact.
    /// The color is calculated from the contact's email address
    /// and can be used for an fallback avatar with white initials
    /// as well as for headlines in bubbles of group chats.
    pub fn get_color(&self) -> u32 {
        dc_str_to_color(&self.addr)
    }

    /// Check if a contact was verified. E.g. by a secure-join QR code scan
    /// and if the key has not changed since this verification.
    ///
    /// The UI may draw a checkbox or something like that beside verified contacts.
    ///
    pub fn is_verified(&self) -> VerifiedStatus {
        self.is_verified_ex(None)
    }

    /// Same as `Contact::is_verified` but allows speeding up things
    /// by adding the peerstate belonging to the contact.
    /// If you do not have the peerstate available, it is loaded automatically.
    pub fn is_verified_ex(&self, peerstate: Option<&Peerstate<'a>>) -> VerifiedStatus {
        // We're always sort of secured-verified as we could verify the key on this device any time with the key
        // on this device
        if self.id == DC_CONTACT_ID_SELF as u32 {
            return VerifiedStatus::BidirectVerified;
        }

        if let Some(peerstate) = peerstate {
            if peerstate.verified_key().is_some() {
                return VerifiedStatus::BidirectVerified;
            }
        }

        let peerstate = Peerstate::from_addr(self.context, &self.context.sql, &self.addr);
        if let Some(ps) = peerstate {
            if ps.verified_key().is_some() {
                return VerifiedStatus::BidirectVerified;
            }
        }

        VerifiedStatus::Unverified
    }

    pub fn addr_equals_contact(context: &Context, addr: impl AsRef<str>, contact_id: u32) -> bool {
        if addr.as_ref().is_empty() {
            return false;
        }

        if let Ok(contact) = Contact::load_from_db(context, contact_id) {
            if !contact.addr.is_empty() {
                let normalized_addr = addr_normalize(addr.as_ref());
                if &contact.addr == &normalized_addr {
                    return true;
                }
            }
        }

        false
    }

    pub fn get_real_cnt(context: &Context) -> usize {
        if !context.sql.is_open() {
            return 0;
        }

        context
            .sql
            .query_row_col::<_, isize>(
                context,
                "SELECT COUNT(*) FROM contacts WHERE id>?;",
                params![DC_CONTACT_ID_LAST_SPECIAL as i32],
                0,
            )
            .unwrap_or_default() as usize
    }

    pub fn get_origin_by_id(context: &Context, contact_id: u32, ret_blocked: &mut i32) -> Origin {
        let mut ret = Origin::Unknown;
        *ret_blocked = 0;

        if let Ok(contact) = Contact::load_from_db(context, contact_id) {
            /* we could optimize this by loading only the needed fields */
            if contact.blocked {
                *ret_blocked = 1;
            } else {
                ret = contact.origin;
            }
        }

        ret
    }

    pub fn real_exists_by_id(context: &Context, contact_id: u32) -> bool {
        if !context.sql.is_open() || contact_id <= 9 {
            return false;
        }

        context
            .sql
            .exists(
                "SELECT id FROM contacts WHERE id=?;",
                params![contact_id as i32],
            )
            .unwrap_or_default()
    }

    pub fn scaleup_origin_by_id(context: &Context, contact_id: u32, origin: Origin) -> bool {
        context
            .sql
            .execute(
                "UPDATE contacts SET origin=? WHERE id=? AND origin<?;",
                params![origin, contact_id as i32, origin],
            )
            .is_ok()
    }
}

fn get_first_name<'a>(full_name: &'a str) -> &'a str {
    full_name.splitn(2, ' ').next().unwrap_or_default()
}

/// Returns false if addr is an invalid address, otherwise true.
pub fn may_be_valid_addr(addr: &str) -> bool {
    let res = addr.parse::<EmailAddress>();
    res.is_ok()
}

pub fn addr_normalize(addr: &str) -> &str {
    let norm = addr.trim();

    if norm.starts_with("mailto:") {
        return &norm[7..];
    }

    norm
}

fn set_block_contact(context: &Context, contact_id: u32, new_blocking: bool) {
    if contact_id <= 9 {
        return;
    }

    if let Ok(contact) = Contact::load_from_db(context, contact_id) {
        if contact.blocked != new_blocking {
            if sql::execute(
                context,
                &context.sql,
                "UPDATE contacts SET blocked=? WHERE id=?;",
                params![new_blocking as i32, contact_id as i32],
            )
            .is_ok()
            {
                // also (un)block all chats with _only_ this contact - we do not delete them to allow a
                // non-destructive blocking->unblocking.
                // (Maybe, beside normal chats (type=100) we should also block group chats with only this user.
                // However, I'm not sure about this point; it may be confusing if the user wants to add other people;
                // this would result in recreating the same group...)
                if sql::execute(
                    context,
                    &context.sql,
                    "UPDATE chats SET blocked=? WHERE type=? AND id IN (SELECT chat_id FROM chats_contacts WHERE contact_id=?);",
                    params![new_blocking, 100, contact_id as i32],
                ).is_ok() {
                    Contact::mark_noticed(context, contact_id);
                    context.call_cb(
                        Event::CONTACTS_CHANGED,
                        0,
                        0,
                    );
                }
            }
        }
    }
}

/// Normalize a name.
///
/// - Remove quotes (come from some bad MUA implementations)
/// - Convert names as "Petersen, Björn" to "Björn Petersen"
/// - Trims the resulting string
///
/// Typically, this function is not needed as it is called implicitly by `Contact::add_address_book`.
pub fn normalize_name(full_name: impl AsRef<str>) -> String {
    let mut full_name = full_name.as_ref().trim();
    if full_name.is_empty() {
        return full_name.into();
    }

    let len = full_name.len();
    if len > 0 {
        let firstchar = full_name.as_bytes()[0];
        let lastchar = full_name.as_bytes()[len - 1];
        if firstchar == '\'' as u8 && lastchar == '\'' as u8
            || firstchar == '\"' as u8 && lastchar == '\"' as u8
            || firstchar == '<' as u8 && lastchar == '>' as u8
        {
            full_name = &full_name[1..len - 1];
        }
    }

    if let Some(p1) = full_name.find(',') {
        let (last_name, first_name) = full_name.split_at(p1);

        let last_name = last_name.trim();
        let first_name = (&first_name[1..]).trim();

        return format!("{} {}", first_name, last_name);
    }

    full_name.trim().into()
}

fn cat_fingerprint(
    ret: &mut String,
    addr: impl AsRef<str>,
    fingerprint_verified: impl AsRef<str>,
    fingerprint_unverified: impl AsRef<str>,
) {
    *ret += &format!(
        "\n\n{}:\n{}",
        addr.as_ref(),
        if !fingerprint_verified.as_ref().is_empty() {
            fingerprint_verified.as_ref()
        } else {
            fingerprint_unverified.as_ref()
        },
    );
    if !fingerprint_verified.as_ref().is_empty()
        && !fingerprint_unverified.as_ref().is_empty()
        && fingerprint_verified.as_ref() != fingerprint_unverified.as_ref()
    {
        *ret += &format!(
            "\n\n{} (alternative):\n{}",
            addr.as_ref(),
            fingerprint_unverified.as_ref()
        );
    }
}

pub fn addr_cmp(addr1: impl AsRef<str>, addr2: impl AsRef<str>) -> bool {
    let norm1 = addr_normalize(addr1.as_ref());
    let norm2 = addr_normalize(addr2.as_ref());

    norm1 == norm2
}

pub fn addr_equals_self(context: &Context, addr: impl AsRef<str>) -> bool {
    if !addr.as_ref().is_empty() {
        let normalized_addr = addr_normalize(addr.as_ref());
        if let Some(self_addr) = context.get_config(Config::ConfiguredAddr) {
            return normalized_addr == self_addr;
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_may_be_valid_addr() {
        assert_eq!(may_be_valid_addr(""), false);
        assert_eq!(may_be_valid_addr("user@domain.tld"), true);
        assert_eq!(may_be_valid_addr("uuu"), false);
        assert_eq!(may_be_valid_addr("dd.tt"), false);
        assert_eq!(may_be_valid_addr("tt.dd@uu"), false);
        assert_eq!(may_be_valid_addr("u@d"), false);
        assert_eq!(may_be_valid_addr("u@d."), false);
        assert_eq!(may_be_valid_addr("u@d.t"), false);
        assert_eq!(may_be_valid_addr("u@d.tt"), true);
        assert_eq!(may_be_valid_addr("u@.tt"), false);
        assert_eq!(may_be_valid_addr("@d.tt"), false);
        assert_eq!(may_be_valid_addr("u.u@d.tt"), true);
    }

    #[test]
    fn test_normalize_name() {
        assert_eq!(&normalize_name("Doe, John"), "John Doe");
        assert_eq!(&normalize_name(" hello world   "), "hello world");
    }

    #[test]
    fn test_normalize_addr() {
        assert_eq!(addr_normalize("mailto:john@doe.com"), "john@doe.com");
        assert_eq!(addr_normalize("  hello@world.com   "), "hello@world.com");
    }

    #[test]
    fn test_get_first_name() {
        assert_eq!(get_first_name("John Doe"), "John");
    }
}
