//! # Imap handling module
//!
//! uses [async-email/async-imap](https://github.com/async-email/async-imap)
//! to implement connect, fetch, delete functionality with standard IMAP servers.

use std::sync::atomic::{AtomicBool, Ordering};

use num_traits::FromPrimitive;

use async_imap::{
    error::Result as ImapResult,
    types::{Capability, Fetch, Flag, Mailbox, Name, NameAttribute},
};
use async_std::sync::{Mutex, RwLock};
use async_std::task;
pub use imap_proto::types::Address;

use crate::coi::CoiConfig;
use crate::config::*;
use crate::constants::*;
use crate::context::Context;
use crate::dc_receive_imf::{
    dc_receive_imf, from_field_to_contact_id, is_msgrmsg_rfc724_mid_in_list,
};
use crate::events::Event;
use crate::headerdef::{HeaderDef, HeaderDefMap};
use crate::job::{job_add, Action};
use crate::login_param::{AuthScheme, CertificateChecks, LoginParam, ServerSecurity, Service};
use crate::message::{self, update_server_uid};
use crate::oauth2::dc_get_oauth2_access_token;
use crate::param::Params;
use crate::stock::StockMessage;
use crate::webpush::WebPushConfig;

mod client;
mod idle;
pub mod metadata;
pub mod select_folder;
mod session;

use client::Client;
use session::Session;

type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, Fail)]
pub enum Error {
    #[fail(display = "IMAP Connect without configured params")]
    ConnectWithoutConfigure,

    #[fail(display = "IMAP Connection Failed params: {}", _0)]
    ConnectionFailed(String),

    #[fail(display = "IMAP No Connection established")]
    NoConnection,

    #[fail(display = "IMAP Could not get OAUTH token")]
    OauthError,

    #[fail(display = "IMAP Could not login as {}", _0)]
    LoginFailed(String),

    #[fail(display = "IMAP Could not fetch")]
    FetchFailed(#[cause] async_imap::error::Error),

    #[fail(display = "IMAP operation attempted while it is torn down")]
    InTeardown,

    #[fail(display = "IMAP operation attempted while it is torn down")]
    SqlError(#[cause] crate::sql::Error),

    #[fail(display = "IMAP got error from elsewhere")]
    WrappedError(#[cause] crate::error::Error),

    #[fail(display = "IMAP select folder error")]
    SelectFolderError(#[cause] select_folder::Error),

    #[fail(display = "Mail parse error")]
    MailParseError(#[cause] mailparse::MailParseError),

    #[fail(display = "No mailbox selected, folder: {:?}", _0)]
    NoMailbox(String),

    #[fail(display = "IMAP other error: {:?}", _0)]
    Other(String),
}

impl From<crate::sql::Error> for Error {
    fn from(err: crate::sql::Error) -> Error {
        Error::SqlError(err)
    }
}

impl From<crate::error::Error> for Error {
    fn from(err: crate::error::Error) -> Error {
        Error::WrappedError(err)
    }
}

impl From<Error> for crate::error::Error {
    fn from(err: Error) -> crate::error::Error {
        crate::error::Error::Message(err.to_string())
    }
}

impl From<select_folder::Error> for Error {
    fn from(err: select_folder::Error) -> Error {
        Error::SelectFolderError(err)
    }
}

impl From<mailparse::MailParseError> for Error {
    fn from(err: mailparse::MailParseError) -> Error {
        Error::MailParseError(err)
    }
}

#[derive(Debug, Display, Clone, Copy, PartialEq, Eq)]
pub enum ImapActionResult {
    Failed,
    RetryLater,
    AlreadyDone,
    Success,
}

/// Prefetch:
/// - Message-ID to check if we already have the message.
/// - In-Reply-To and References to check if message is a reply to chat message.
/// - Chat-Version to check if a message is a chat message
/// - Autocrypt-Setup-Message to check if a message is an autocrypt setup message,
///   not necessarily sent by Delta Chat.
const PREFETCH_FLAGS: &str = "(UID BODY.PEEK[HEADER.FIELDS (\
                              MESSAGE-ID \
                              FROM \
                              IN-REPLY-TO REFERENCES \
                              CHAT-VERSION \
                              AUTOCRYPT-SETUP-MESSAGE\
                              )])";
const DELETE_CHECK_FLAGS: &str = "(UID BODY.PEEK[HEADER.FIELDS (MESSAGE-ID)])";
const JUST_UID: &str = "(UID)";
const BODY_FLAGS: &str = "(FLAGS BODY.PEEK[])";
const SELECT_ALL: &str = "1:*";

#[derive(Debug, Default)]
pub struct Imap {
    config: RwLock<ImapConfig>,
    session: Mutex<Option<Session>>,
    connected: Mutex<bool>,
    interrupt: Mutex<Option<stop_token::StopSource>>,
    skip_next_idle_wait: AtomicBool,
    should_reconnect: AtomicBool,
}

#[derive(Debug)]
struct OAuth2 {
    user: String,
    access_token: String,
}

impl async_imap::Authenticator for OAuth2 {
    type Response = String;

    fn process(&self, _data: &[u8]) -> Self::Response {
        format!(
            "user={}\x01auth=Bearer {}\x01\x01",
            self.user, self.access_token
        )
    }
}

#[derive(Debug)]
enum FolderMeaning {
    Unknown,
    SentObjects,
    Other,
}

#[derive(Debug)]
struct ImapConfig {
    pub addr: String,
    pub imap_server: String,
    pub imap_port: u16,
    pub imap_user: String,
    pub imap_pw: String,
    pub certificate_checks: CertificateChecks,
    pub server_security: ServerSecurity,
    pub selected_folder: Option<String>,
    pub selected_mailbox: Option<Mailbox>,
    pub selected_folder_needs_expunge: bool,
    pub can_idle: bool,
    pub auth_scheme: AuthScheme,

    /// True if the server has MOVE capability as defined in
    /// https://tools.ietf.org/html/rfc6851
    pub can_move: bool,
    pub coi: Option<CoiConfig>,
    pub webpush: Option<WebPushConfig>,
}

impl Default for ImapConfig {
    fn default() -> Self {
        ImapConfig {
            addr: "".into(),
            imap_server: "".into(),
            imap_port: 0,
            imap_user: "".into(),
            imap_pw: "".into(),
            certificate_checks: Default::default(),
            server_security: ServerSecurity::PlainSocket,
            auth_scheme: AuthScheme::Plain,
            selected_folder: None,
            selected_mailbox: None,
            selected_folder_needs_expunge: false,
            can_idle: false,
            can_move: false,
            coi: None,
            webpush: None,
        }
    }
}

impl Imap {
    pub fn new() -> Self {
        Default::default()
    }

    pub async fn is_connected(&self) -> bool {
        *self.connected.lock().await
    }

    pub fn should_reconnect(&self) -> bool {
        self.should_reconnect.load(Ordering::Relaxed)
    }

    pub fn trigger_reconnect(&self) {
        self.should_reconnect.store(true, Ordering::Relaxed)
    }

    async fn setup_handle_if_needed(&self, context: &Context) -> Result<()> {
        if self.config.read().await.imap_server.is_empty() {
            return Err(Error::InTeardown);
        }

        if self.should_reconnect() {
            self.unsetup_handle(context).await;
            self.should_reconnect.store(false, Ordering::Relaxed);
        } else if self.is_connected().await {
            return Ok(());
        }

        let connection_res: ImapResult<Client> = {
            let config = self.config.read().await;
            let server_security = config.server_security;
            let imap_server: &str = config.imap_server.as_ref();
            let imap_port = config.imap_port;
            match server_security  {
                ServerSecurity::Ssl => {
                    Client::connect_secure(
                        (imap_server, imap_port),
                        imap_server,
                        config.certificate_checks,
                    )
                        .await
                },
                _ => {
                    match Client::connect_insecure((imap_server, imap_port)).await {
                        Ok(client) => {
                            if server_security == ServerSecurity::Starttls {
                                client.secure(imap_server, config.certificate_checks).await
                            } else {
                                Ok(client)
                            }
                        }
                        Err(err) => Err(err),
                    }
                },
            }
        };

        let login_res = {
            let config = self.config.read().await;
            let imap_user: &str = config.imap_user.as_ref();
            let imap_pw: &str = config.imap_pw.as_ref();
            let auth_scheme = config.auth_scheme;
            match connection_res {
                Ok(client) => {
                    if auth_scheme == AuthScheme::Oauth2 {
                        let addr: &str = config.addr.as_ref();

                        if let Some(token) = dc_get_oauth2_access_token(context, addr, imap_pw, true) {
                            let auth = OAuth2 {
                                user: imap_user.into(),
                                access_token: token,
                            };
                            client.authenticate("XOAUTH2", &auth).await
                        } else {
                            return Err(Error::OauthError);
                        }
                    } else {
                        client.login(imap_user, imap_pw).await
                    }
                }
                Err(err) => {
                    let message = {
                        let config = self.config.read().await;
                        let imap_server: &str = config.imap_server.as_ref();
                        let imap_port = config.imap_port;
                        context.stock_string_repl_str2(
                            StockMessage::ServerResponse,
                            format!("{}:{}", imap_server, imap_port),
                            err.to_string(),
                        )
                    };
                    // IMAP connection failures are reported to users
                    emit_event!(context, Event::ErrorNetwork(message));
                    return Err(Error::ConnectionFailed(err.to_string()));
                }
            }
        };

        self.should_reconnect.store(false, Ordering::Relaxed);

        match login_res {
            Ok(session) => {
                *self.session.lock().await = Some(session);
                Ok(())
            }
            Err((err, _)) => {
                let imap_user = self.config.read().await.imap_user.to_owned();
                let message = context.stock_string_repl_str(StockMessage::CannotLogin, &imap_user);

                emit_event!(
                    context,
                    Event::ErrorNetwork(format!("{} ({})", message, err))
                );
                self.trigger_reconnect();
                Err(Error::LoginFailed(format!("cannot login as {}", imap_user)))
            }
        }
    }

    async fn unsetup_handle(&self, context: &Context) {
        info!(
            context,
            "IMAP unsetup_handle step 2 (acquiring session.lock)"
        );
        if let Some(mut session) = self.session.lock().await.take() {
            if let Err(err) = session.close().await {
                warn!(context, "failed to close connection: {:?}", err);
            }
        }
        *self.connected.lock().await = false;

        info!(context, "IMAP unsetup_handle step 3 (clearing config).");
        self.config.write().await.selected_folder = None;
        self.config.write().await.selected_mailbox = None;
        info!(context, "IMAP unsetup_handle step 4 (disconnected)");
    }

    async fn free_connect_params(&self) {
        let mut cfg = self.config.write().await;

        cfg.addr = "".into();
        cfg.imap_server = "".into();
        cfg.imap_user = "".into();
        cfg.imap_pw = "".into();
        cfg.imap_port = 0;

        cfg.can_idle = false;
        cfg.can_move = false;
    }
    
    /// Connects to imap account using already-configured parameters.
    pub fn connect_configured(&self, context: &Context) -> Result<()> {
        if async_std::task::block_on(self.is_connected()) && !self.should_reconnect() {
            return Ok(());
        }
        if !context.sql.get_raw_config_bool(context, "configured") {
            return Err(Error::ConnectWithoutConfigure);
        }

        let param = LoginParam::from_database(context, "configured_");
        // the trailing underscore is correct

        if task::block_on(self.connect(context, &param)) {
            self.ensure_configured_folders(context, true)
        } else {
            Err(Error::ConnectionFailed(format!("{}", param)))
        }
    }

    /// tries connecting to imap account using the specific login
    /// parameters
    pub async fn connect(&self, context: &Context, lp: &LoginParam) -> bool {
        if lp.srv_params[Service::Imap as usize].hostname.is_empty() || lp.srv_params[Service::Imap as usize].user.is_empty() || lp.srv_params[Service::Imap as usize].pw.is_empty() {
            context.call_cb(Event::ErrorNetwork("IMAP bad parameters.".into()));
            return false;
        }

        {
            let addr = &lp.addr;
            let imap_server = &lp.srv_params[Service::Imap as usize].hostname;
            let imap_port = lp.srv_params[Service::Imap as usize].port as u16;
            let imap_user = &lp.srv_params[Service::Imap as usize].user;
            let imap_pw = &lp.srv_params[Service::Imap as usize].pw;
            let server_security = lp.srv_params[Service::Imap as usize].security;
            let auth_scheme = lp.auth_scheme;

            let mut config = self.config.write().await;
            config.addr = addr.to_string();
            config.imap_server = imap_server.to_string();
            config.imap_port = imap_port;
            config.imap_user = imap_user.to_string();
            config.imap_pw = imap_pw.to_string();
            config.certificate_checks = lp.srv_params[Service::Imap as usize].certificate_checks;
            config.server_security = server_security.unwrap();
            config.auth_scheme = auth_scheme;
        }

        if let Err(err) = self.setup_handle_if_needed(context).await {
            warn!(context, "failed to setup imap handle: {}", err);
            self.free_connect_params().await;
            return false;
        }

        let mut has_coi = false;
        let mut has_webpush = false;
        let teardown = match &mut *self.session.lock().await {
            Some(ref mut session) => match session.capabilities().await {
                Ok(caps) => {
                    if !context.sql.is_open() {
                        warn!(context, "IMAP-LOGIN as {} ok but ABORTING", lp.srv_params[Service::Imap as usize].user,);
                        true
                    } else {
                        let can_idle = caps.has_str("IDLE");
                        let can_move = caps.has_str("MOVE");
                        has_coi = caps.has(&Capability::Atom(String::from("COI")));
                        has_webpush = caps.has(&Capability::Atom(String::from("WEBPUSH")));
                        let caps_list = caps.iter().fold(String::new(), |s, c| {
                            if let Capability::Atom(x) = c {
                                s + &format!(" {}", x)
                            } else {
                                s + &format!(" {:?}", c)
                            }
                        });

                        self.config.write().await.can_idle = can_idle;
                        self.config.write().await.can_move = can_move;
                        *self.connected.lock().await = true;
                        emit_event!(
                            context,
                            Event::ImapConnected(format!(
                                "IMAP-LOGIN as {}, capabilities: {}",
                                lp.srv_params[Service::Imap as usize].user, caps_list,
                            ))
                        );
                        false
                    }
                }
                Err(err) => {
                    info!(context, "CAPABILITY command error: {}", err);
                    true
                }
            },
            None => true,
        };

        if teardown {
            self.disconnect(context);
            false
        } else {
            self.update_metadata(context, Some(has_coi), Some(has_webpush))
                .await;
            true
        }
    }

    pub fn disconnect(&self, context: &Context) {
        task::block_on(async move {
            self.unsetup_handle(context).await;
            self.free_connect_params().await;
        });
    }

    pub async fn fetch(&self, context: &Context, watch_folder: &str) -> Result<()> {
        if !context.sql.is_open() {
            // probably shutdown
            return Err(Error::InTeardown);
        }
        self.setup_handle_if_needed(context).await?;

        while self.fetch_new_messages(context, &watch_folder).await? {
            // We fetch until no more new messages are there.
        }
        Ok(())
    }

    fn get_config_last_seen_uid<S: AsRef<str>>(&self, context: &Context, folder: S) -> (u32, u32) {
        let key = format!("imap.mailbox.{}", folder.as_ref());
        if let Some(entry) = context.sql.get_raw_config(context, &key) {
            // the entry has the format `imap.mailbox.<folder>=<uidvalidity>:<lastseenuid>`
            let mut parts = entry.split(':');
            (
                parts
                    .next()
                    .unwrap_or_default()
                    .parse()
                    .unwrap_or_else(|_| 0),
                parts
                    .next()
                    .unwrap_or_default()
                    .parse()
                    .unwrap_or_else(|_| 0),
            )
        } else {
            (0, 0)
        }
    }

    /// return Result with (uid_validity, last_seen_uid) tuple.
    pub(crate) fn select_with_uidvalidity(
        &self,
        context: &Context,
        folder: &str,
    ) -> Result<(u32, u32)> {
        task::block_on(async move {
            self.select_folder(context, Some(folder)).await?;

            // compare last seen UIDVALIDITY against the current one
            let (uid_validity, last_seen_uid) = self.get_config_last_seen_uid(context, &folder);

            let config = self.config.read().await;
            let mailbox = config
                .selected_mailbox
                .as_ref()
                .ok_or_else(|| Error::NoMailbox(folder.to_string()))?;

            let new_uid_validity = match mailbox.uid_validity {
                Some(v) => v,
                None => {
                    let s = format!("No UIDVALIDITY for folder {:?}", folder);
                    return Err(Error::Other(s));
                }
            };

            if new_uid_validity == uid_validity {
                return Ok((uid_validity, last_seen_uid));
            }

            if mailbox.exists == 0 {
                info!(context, "Folder \"{}\" is empty.", folder);

                // set lastseenuid=0 for empty folders.
                // id we do not do this here, we'll miss the first message
                // as we will get in here again and fetch from lastseenuid+1 then

                self.set_config_last_seen_uid(context, &folder, new_uid_validity, 0);
                return Ok((new_uid_validity, 0));
            }

            // uid_validity has changed or is being set the first time.
            // find the last seen uid within the new uid_validity scope.
            let new_last_seen_uid = match mailbox.uid_next {
                Some(uid_next) => {
                    uid_next - 1 // XXX could uid_next be 0?
                }
                None => {
                    warn!(
                        context,
                        "IMAP folder has no uid_next, fall back to fetching"
                    );
                    if let Some(ref mut session) = &mut *self.session.lock().await {
                        // note that we use fetch by sequence number
                        // and thus we only need to get exactly the
                        // last-index message.
                        let set = format!("{}", mailbox.exists);
                        match session.fetch(set, JUST_UID).await {
                            Ok(list) => list[0].uid.unwrap_or_default(),
                            Err(err) => {
                                return Err(Error::FetchFailed(err));
                            }
                        }
                    } else {
                        return Err(Error::NoConnection);
                    }
                }
            };

            self.set_config_last_seen_uid(context, &folder, new_uid_validity, new_last_seen_uid);
            info!(
                context,
                "uid/validity change: new {}/{} current {}/{}",
                new_last_seen_uid,
                new_uid_validity,
                uid_validity,
                last_seen_uid
            );
            Ok((new_uid_validity, new_last_seen_uid))
        })
    }

    async fn fetch_new_messages<S: AsRef<str>>(
        &self,
        context: &Context,
        folder: S,
    ) -> Result<bool> {
        let show_emails =
            ShowEmails::from_i32(context.get_config_int(Config::ShowEmails)).unwrap_or_default();

        let (uid_validity, last_seen_uid) =
            self.select_with_uidvalidity(context, folder.as_ref())?;

        let mut read_cnt = 0;

        let mut list = if let Some(ref mut session) = &mut *self.session.lock().await {
            // fetch messages with larger UID than the last one seen
            // `(UID FETCH lastseenuid+1:*)`, see RFC 4549
            let set = format!("{}:*", last_seen_uid + 1);
            match session.uid_fetch(set, PREFETCH_FLAGS).await {
                Ok(list) => list,
                Err(err) => {
                    return Err(Error::FetchFailed(err));
                }
            }
        } else {
            return Err(Error::NoConnection);
        };

        // prefetch info from all unfetched mails
        let mut new_last_seen_uid = last_seen_uid;
        let mut read_errors = 0;

        list.sort_unstable_by_key(|msg| msg.uid.unwrap_or_default());

        for fetch in &list {
            let cur_uid = fetch.uid.unwrap_or_default();
            if cur_uid <= last_seen_uid {
                // If the mailbox is not empty, results always include
                // at least one UID, even if last_seen_uid+1 is past
                // the last UID in the mailbox.  It happens because
                // uid+1:* is interpreted the same way as *:uid+1.
                // See https://tools.ietf.org/html/rfc3501#page-61 for
                // standard reference. Therefore, sometimes we receive
                // already seen messages and have to filter them out.
                info!(
                    context,
                    "fetch_new_messages: ignoring uid {}, last seen was {}", cur_uid, last_seen_uid
                );
                continue;
            }
            read_cnt += 1;

            let headers = get_fetch_headers(fetch)?;
            let message_id = prefetch_get_message_id(&headers).unwrap_or_default();
            if precheck_imf(context, &message_id, folder.as_ref(), cur_uid) {
                // we know the message-id already or don't want the message otherwise.
                info!(
                    context,
                    "Skipping message {} from \"{}\" by precheck.",
                    message_id,
                    folder.as_ref(),
                );
            } else {
                let show = prefetch_should_download(context, &headers, show_emails)
                    .map_err(|err| {
                        warn!(context, "prefetch_should_download error: {}", err);
                        err
                    })
                    .unwrap_or(true);

                if !show {
                    info!(
                        context,
                        "Ignoring new message {} from \"{}\".",
                        message_id,
                        folder.as_ref(),
                    );
                } else {
                    // check passed, go fetch the rest
                    if let Err(err) = self
                        .fetch_single_msg(
                            context,
                            &folder,
                            cur_uid,
                            headers
                                .get_header_value(HeaderDef::From_)?
                                .unwrap_or(String::from("")),
                        )
                        .await
                    {
                        info!(
                            context,
                            "Read error for message {} from \"{}\", trying over later: {}.",
                            message_id,
                            folder.as_ref(),
                            err
                        );
                        read_errors += 1;
                    }
                }
            }
            if read_errors == 0 {
                new_last_seen_uid = cur_uid;
            }
        }

        if new_last_seen_uid > last_seen_uid {
            self.set_config_last_seen_uid(context, &folder, uid_validity, new_last_seen_uid);
        }

        if read_errors > 0 {
            warn!(
                context,
                "{} mails read from \"{}\" with {} errors.",
                read_cnt,
                folder.as_ref(),
                read_errors
            );
        } else {
            info!(
                context,
                "{} mails read from \"{}\".",
                read_cnt,
                folder.as_ref()
            );
        }

        Ok(read_cnt > 0)
    }

    fn set_config_last_seen_uid<S: AsRef<str>>(
        &self,
        context: &Context,
        folder: S,
        uidvalidity: u32,
        lastseenuid: u32,
    ) {
        let key = format!("imap.mailbox.{}", folder.as_ref());
        let val = format!("{}:{}", uidvalidity, lastseenuid);

        context.sql.set_raw_config(context, &key, Some(&val)).ok();
    }

    /// Fetches a single message by server UID.
    ///
    /// If it succeeds, the message should be treated as received even
    /// if no database entries are created. If the function returns an
    /// error, the caller should try again later.
    async fn fetch_single_msg<S: AsRef<str>>(
        &self,
        context: &Context,
        folder: S,
        server_uid: u32,
        from: String,
    ) -> Result<()> {
        if !self.is_connected().await {
            return Err(Error::Other("Not connected".to_string()));
        }

        let set = format!("{}", server_uid);

        let msgs = if let Some(ref mut session) = &mut *self.session.lock().await {
            match session.uid_fetch(set, BODY_FLAGS).await {
                Ok(msgs) => msgs,
                Err(err) => {
                    // TODO maybe differentiate between IO and input/parsing problems
                    // so we don't reconnect if we have a (rare) input/output parsing problem?
                    self.trigger_reconnect();
                    warn!(
                        context,
                        "Error on fetching message #{} from folder \"{}\"; error={}.",
                        server_uid,
                        folder.as_ref(),
                        err
                    );
                    return Err(Error::FetchFailed(err));
                }
            }
        } else {
            // we could not get a valid imap session, this should be retried
            self.trigger_reconnect();
            return Err(Error::Other("Could not get IMAP session".to_string()));
        };

        if let Some(msg) = msgs.first() {
            // XXX put flags into a set and pass them to dc_receive_imf
            let is_deleted = msg.flags().any(|flag| flag == Flag::Deleted);
            let is_seen = msg.flags().any(|flag| flag == Flag::Seen);

            if !is_deleted && msg.body().is_some() {
                let body = msg.body().unwrap_or_default();
                if let Err(err) =
                    dc_receive_imf(context, &body, folder.as_ref(), server_uid, is_seen)
                {
                    match err {
                        crate::error::Error::Pgp(pgp::errors::Error::MissingKey) => {
                            context.call_cb(Event::MissingKey(from));
                        }
                        _ => {
                            warn!(
                                context,
                                "dc_receive_imf failed for imap-message {}/{}: {:?}",
                                folder.as_ref(),
                                server_uid,
                                err
                            );
                        }
                    }
                }
            }
        } else {
            warn!(
                context,
                "Message #{} does not exist in folder \"{}\".",
                server_uid,
                folder.as_ref()
            );
        }

        Ok(())
    }

    pub fn can_move(&self) -> bool {
        task::block_on(async move { self.config.read().await.can_move })
    }

    pub fn mv(
        &self,
        context: &Context,
        folder: &str,
        uid: u32,
        dest_folder: &str,
        dest_uid: &mut u32,
    ) -> ImapActionResult {
        task::block_on(async move {
            if folder == dest_folder {
                info!(
                    context,
                    "Skip moving message; message {}/{} is already in {}...",
                    folder,
                    uid,
                    dest_folder,
                );
                return ImapActionResult::AlreadyDone;
            }
            if let Some(imapresult) = self.prepare_imap_operation_on_msg(context, folder, uid) {
                return imapresult;
            }
            // we are connected, and the folder is selected

            // XXX Rust-Imap provides no target uid on mv, so just set it to 0
            *dest_uid = 0;

            let set = format!("{}", uid);
            let display_folder_id = format!("{}/{}", folder, uid);

            if self.can_move() {
                if let Some(ref mut session) = &mut *self.session.lock().await {
                    match session.uid_mv(&set, &dest_folder).await {
                        Ok(_) => {
                            emit_event!(
                                context,
                                Event::ImapMessageMoved(format!(
                                    "IMAP Message {} moved to {}",
                                    display_folder_id, dest_folder
                                ))
                            );
                            return ImapActionResult::Success;
                        }
                        Err(err) => {
                            warn!(
                                context,
                                "Cannot move message, fallback to COPY/DELETE {}/{} to {}: {}",
                                folder,
                                uid,
                                dest_folder,
                                err
                            );
                        }
                    }
                } else {
                    unreachable!();
                };
            } else {
                info!(
                    context,
                    "Server does not support MOVE, fallback to COPY/DELETE {}/{} to {}",
                    folder,
                    uid,
                    dest_folder
                );
            }

            if let Some(ref mut session) = &mut *self.session.lock().await {
                if let Err(err) = session.uid_copy(&set, &dest_folder).await {
                    warn!(context, "Could not copy message: {}", err);
                    return ImapActionResult::Failed;
                }
            } else {
                unreachable!();
            }

            if !self.add_flag_finalized(context, uid, "\\Deleted").await {
                warn!(context, "Cannot mark {} as \"Deleted\" after copy.", uid);
                emit_event!(
                    context,
                    Event::ImapMessageMoved(format!(
                        "IMAP Message {} copied to {} (delete FAILED)",
                        display_folder_id, dest_folder
                    ))
                );
                ImapActionResult::Failed
            } else {
                self.config.write().await.selected_folder_needs_expunge = true;
                emit_event!(
                    context,
                    Event::ImapMessageMoved(format!(
                        "IMAP Message {} copied to {} (delete successfull)",
                        display_folder_id, dest_folder
                    ))
                );
                ImapActionResult::Success
            }
        })
    }

    async fn add_flag_finalized(&self, context: &Context, server_uid: u32, flag: &str) -> bool {
        // return true if we successfully set the flag or we otherwise
        // think add_flag should not be retried: Disconnection during setting
        // the flag, or other imap-errors, returns true as well.
        //
        // returning false means that the operation can be retried.
        if server_uid == 0 {
            return true; // might be moved but we don't want to have a stuck job
        }
        let s = server_uid.to_string();
        self.add_flag_finalized_with_set(context, &s, flag).await
    }

    async fn add_flag_finalized_with_set(
        &self,
        context: &Context,
        uid_set: &str,
        flag: &str,
    ) -> bool {
        if self.should_reconnect() {
            return false;
        }
        if let Some(ref mut session) = &mut *self.session.lock().await {
            let query = format!("+FLAGS ({})", flag);
            match session.uid_store(uid_set, &query).await {
                Ok(_) => {}
                Err(err) => {
                    warn!(
                        context,
                        "IMAP failed to store: ({}, {}) {:?}", uid_set, query, err
                    );
                }
            }
            true // we tried once, that's probably enough for setting flag
        } else {
            unreachable!();
        }
    }

    pub fn prepare_imap_operation_on_msg(
        &self,
        context: &Context,
        folder: &str,
        uid: u32,
    ) -> Option<ImapActionResult> {
        task::block_on(async move {
            if uid == 0 {
                return Some(ImapActionResult::Failed);
            }
            if !self.is_connected().await {
                // currently jobs are only performed on the INBOX thread
                // TODO: make INBOX/SENT/MVBOX perform the jobs on their
                // respective folders to avoid select_folder network traffic
                // and the involved error states
                if let Err(err) = self.connect_configured(context) {
                    warn!(context, "prepare_imap_op failed: {}", err);
                    return Some(ImapActionResult::RetryLater);
                }
            }
            match self.select_folder(context, Some(&folder)).await {
                Ok(()) => None,
                Err(select_folder::Error::ConnectionLost) => {
                    warn!(context, "Lost imap connection");
                    Some(ImapActionResult::RetryLater)
                }
                Err(select_folder::Error::NoSession) => {
                    warn!(context, "no imap session");
                    Some(ImapActionResult::Failed)
                }
                Err(select_folder::Error::BadFolderName(folder_name)) => {
                    warn!(context, "invalid folder name: {:?}", folder_name);
                    Some(ImapActionResult::Failed)
                }
                Err(err) => {
                    warn!(context, "failed to select folder: {:?}: {:?}", folder, err);
                    Some(ImapActionResult::RetryLater)
                }
            }
        })
    }

    pub fn set_seen(&self, context: &Context, folder: &str, uid: u32) -> ImapActionResult {
        task::block_on(async move {
            if let Some(imapresult) = self.prepare_imap_operation_on_msg(context, folder, uid) {
                return imapresult;
            }
            // we are connected, and the folder is selected
            info!(context, "Marking message {}/{} as seen...", folder, uid,);

            if self.add_flag_finalized(context, uid, "\\Seen").await {
                ImapActionResult::Success
            } else {
                warn!(
                    context,
                    "Cannot mark message {} in folder {} as seen, ignoring.", uid, folder
                );
                ImapActionResult::Failed
            }
        })
    }

    pub fn delete_msg(
        &self,
        context: &Context,
        message_id: &str,
        folder: &str,
        uid: &mut u32,
    ) -> ImapActionResult {
        task::block_on(async move {
            if let Some(imapresult) = self.prepare_imap_operation_on_msg(context, folder, *uid) {
                return imapresult;
            }
            // we are connected, and the folder is selected

            let set = format!("{}", uid);
            let display_imap_id = format!("{}/{}", folder, uid);

            // double-check that we are deleting the correct message-id
            // this comes at the expense of another imap query
            if let Some(ref mut session) = &mut *self.session.lock().await {
                match session.uid_fetch(set, DELETE_CHECK_FLAGS).await {
                    Ok(msgs) => {
                        let fetch = if let Some(fetch) = msgs.first() {
                            fetch
                        } else {
                            warn!(
                                context,
                                "Cannot delete on IMAP, {}: imap entry gone '{}'",
                                display_imap_id,
                                message_id,
                            );
                            return ImapActionResult::Failed;
                        };

                        let remote_message_id = get_fetch_headers(fetch)
                            .and_then(|headers| prefetch_get_message_id(&headers))
                            .unwrap_or_default();

                        if remote_message_id != message_id {
                            warn!(
                                context,
                                "Cannot delete on IMAP, {}: remote message-id '{}' != '{}'",
                                display_imap_id,
                                remote_message_id,
                                message_id,
                            );
                            *uid = 0;
                        }
                    }
                    Err(err) => {
                        warn!(
                            context,
                            "Cannot delete {} on IMAP: {}", display_imap_id, err
                        );
                        *uid = 0;
                    }
                }
            }

            // mark the message for deletion
            if !self.add_flag_finalized(context, *uid, "\\Deleted").await {
                warn!(
                    context,
                    "Cannot mark message {} as \"Deleted\".", display_imap_id
                );
                ImapActionResult::Failed
            } else {
                emit_event!(
                    context,
                    Event::ImapMessageDeleted(format!(
                        "IMAP Message {} marked as deleted [{}]",
                        display_imap_id, message_id
                    ))
                );
                self.config.write().await.selected_folder_needs_expunge = true;
                ImapActionResult::Success
            }
        })
    }

    pub fn ensure_configured_folders(&self, context: &Context, create_mvbox: bool) -> Result<()> {
        let folders_configured = context
            .sql
            .get_raw_config_int(context, "folders_configured");
        if folders_configured.unwrap_or_default() >= DC_FOLDERS_CONFIGURED_VERSION {
            return Ok(());
        }

        self.configure_folders(context, create_mvbox)
    }

    pub fn configure_folders(&self, context: &Context, create_mvbox: bool) -> Result<()> {
        task::block_on(async move {
            if !self.is_connected().await {
                return Err(Error::NoConnection);
            }

            info!(context, "Configuring IMAP-folders.");

            if let Some(ref mut session) = &mut *self.session.lock().await {
                let folders = match self.list_folders(session, context).await {
                    Some(f) => f,
                    None => {
                        return Err(Error::Other("list_folders failed".to_string()));
                    }
                };

                let sentbox_folder =
                    folders
                        .iter()
                        .find(|folder| match get_folder_meaning(folder) {
                            FolderMeaning::SentObjects => true,
                            _ => false,
                        });
                info!(context, "sentbox folder is {:?}", sentbox_folder);

                let mut delimiter = ".";
                if let Some(folder) = folders.first() {
                    if let Some(d) = folder.delimiter() {
                        if !d.is_empty() {
                            delimiter = d;
                        }
                    }
                }
                info!(context, "Using \"{}\" as folder-delimiter.", delimiter);
                let fallback_folder = format!("INBOX{}DeltaChat", delimiter);

                let mut mvbox_folder = folders
                    .iter()
                    .find(|folder| folder.name() == "DeltaChat" || folder.name() == fallback_folder)
                    .map(|n| n.name().to_string());

                if mvbox_folder.is_none() && create_mvbox {
                    info!(context, "Creating MVBOX-folder \"DeltaChat\"...",);

                    match session.create("DeltaChat").await {
                        Ok(_) => {
                            mvbox_folder = Some("DeltaChat".into());

                            info!(context, "MVBOX-folder created.",);
                        }
                        Err(err) => {
                            warn!(
                                context,
                                "Cannot create MVBOX-folder, trying to create INBOX subfolder. ({})",
                                err
                            );

                            match session.create(&fallback_folder).await {
                                Ok(_) => {
                                    mvbox_folder = Some(fallback_folder);
                                    info!(
                                        context,
                                        "MVBOX-folder created as INBOX subfolder. ({})", err
                                    );
                                }
                                Err(err) => {
                                    warn!(context, "Cannot create MVBOX-folder. ({})", err);
                                }
                            }
                        }
                    }
                    // SUBSCRIBE is needed to make the folder visible to the LSUB command
                    // that may be used by other MUAs to list folders.
                    // for the LIST command, the folder is always visible.
                    if let Some(ref mvbox) = mvbox_folder {
                        if let Err(err) = session.subscribe(mvbox).await {
                            warn!(context, "could not subscribe to {:?}: {:?}", mvbox, err);
                        }
                    }
                }
                context
                    .sql
                    .set_raw_config(context, "configured_inbox_folder", Some("INBOX"))?;
                if let Some(ref mvbox_folder) = mvbox_folder {
                    context.sql.set_raw_config(
                        context,
                        "configured_mvbox_folder",
                        Some(mvbox_folder),
                    )?;
                }
                if let Some(ref sentbox_folder) = sentbox_folder {
                    context.sql.set_raw_config(
                        context,
                        "configured_sentbox_folder",
                        Some(sentbox_folder.name()),
                    )?;
                }
                context.sql.set_raw_config_int(
                    context,
                    "folders_configured",
                    DC_FOLDERS_CONFIGURED_VERSION,
                )?;
            }
            info!(context, "FINISHED configuring IMAP-folders.");
            Ok(())
        })
    }

    async fn list_folders(&self, session: &mut Session, context: &Context) -> Option<Vec<Name>> {
        match session.list(Some(""), Some("*")).await {
            Ok(list) => {
                if list.is_empty() {
                    warn!(context, "Folder list is empty.",);
                }
                Some(list)
            }
            Err(err) => {
                eprintln!("list error: {:?}", err);
                warn!(context, "Cannot get folder list.",);

                None
            }
        }
    }

    pub fn empty_folder(&self, context: &Context, folder: &str) {
        task::block_on(async move {
            info!(context, "emptying folder {}", folder);

            // we want to report all error to the user
            // (no retry should be attempted)
            if folder.is_empty() {
                error!(context, "cannot perform empty, folder not set");
                return;
            }
            if let Err(err) = self.setup_handle_if_needed(context).await {
                error!(context, "could not setup imap connection: {:?}", err);
                return;
            }
            if let Err(err) = self.select_folder(context, Some(&folder)).await {
                error!(
                    context,
                    "Could not select {} for expunging: {:?}", folder, err
                );
                return;
            }

            if !self
                .add_flag_finalized_with_set(context, SELECT_ALL, "\\Deleted")
                .await
            {
                error!(context, "Cannot mark messages for deletion {}", folder);
                return;
            }

            // we now trigger expunge to actually delete messages
            self.config.write().await.selected_folder_needs_expunge = true;
            match self.select_folder::<String>(context, None).await {
                Ok(()) => {
                    emit_event!(context, Event::ImapFolderEmptied(folder.to_string()));
                }
                Err(err) => {
                    error!(context, "expunge failed {}: {:?}", folder, err);
                }
            }

            if let Err(err) = crate::sql::execute(
                context,
                &context.sql,
                "UPDATE msgs SET server_folder='',server_uid=0 WHERE server_folder=?",
                params![folder],
            ) {
                warn!(
                    context,
                    "Failed to reset server_uid and server_folder for deleted messages: {}", err
                );
            }
        });
    }

    pub async fn get_coi_config(&self) -> Option<CoiConfig> {
        self.config.read().await.coi.clone()
    }

    pub async fn get_webpush_config(&self) -> Option<WebPushConfig> {
        self.config.read().await.webpush.clone()
    }
}

// #[async_trait]
// impl TryConnect for Imap {
//     async fn try_connect(&self, context: &Context, lp: &LoginParam) -> bool {
//         self.connect(context, lp).await
//     }
// }

/// Try to get the folder meaning by the name of the folder only used if the server does not support XLIST.
// TODO: lots languages missing - maybe there is a list somewhere on other MUAs?
// however, if we fail to find out the sent-folder,
// only watching this folder is not working. at least, this is no show stopper.
// CAVE: if possible, take care not to add a name here that is "sent" in one language
// but sth. different in others - a hard job.
fn get_folder_meaning_by_name(folder_name: &Name) -> FolderMeaning {
    let sent_names = vec!["sent", "sent objects", "gesendet"];
    let lower = folder_name.name().to_lowercase();

    if sent_names.into_iter().any(|s| s == lower) {
        FolderMeaning::SentObjects
    } else {
        FolderMeaning::Unknown
    }
}

fn get_folder_meaning(folder_name: &Name) -> FolderMeaning {
    if folder_name.attributes().is_empty() {
        return FolderMeaning::Unknown;
    }

    let mut res = FolderMeaning::Unknown;
    let special_names = vec!["\\Spam", "\\Trash", "\\Drafts", "\\Junk"];

    for attr in folder_name.attributes() {
        if let NameAttribute::Custom(ref label) = attr {
            if special_names.iter().any(|s| *s == label) {
                res = FolderMeaning::Other;
            } else if label == "\\Sent" {
                res = FolderMeaning::SentObjects
            }
        }
    }

    match res {
        FolderMeaning::Unknown => get_folder_meaning_by_name(folder_name),
        _ => res,
    }
}

fn precheck_imf(context: &Context, rfc724_mid: &str, server_folder: &str, server_uid: u32) -> bool {
    if let Ok((old_server_folder, old_server_uid, msg_id)) =
        message::rfc724_mid_exists(context, &rfc724_mid)
    {
        if old_server_folder.is_empty() && old_server_uid == 0 {
            info!(context, "[move] detected bcc-self {}", rfc724_mid,);
            context.do_heuristics_moves(server_folder.as_ref(), msg_id);
            job_add(
                context,
                Action::MarkseenMsgOnImap,
                msg_id.to_u32() as i32,
                Params::new(),
                0,
            );
        } else if old_server_folder != server_folder {
            info!(context, "[move] detected moved message {}", rfc724_mid,);
        }

        if old_server_folder != server_folder || old_server_uid != server_uid {
            update_server_uid(context, &rfc724_mid, server_folder, server_uid);
        }
        true
    } else {
        false
    }
}

fn get_fetch_headers(prefetch_msg: &Fetch) -> Result<Vec<mailparse::MailHeader>> {
    let header_bytes = match prefetch_msg.header() {
        Some(header_bytes) => header_bytes,
        None => return Ok(Vec::new()),
    };
    let (headers, _) = mailparse::parse_headers(header_bytes)?;
    Ok(headers)
}

fn prefetch_get_message_id(headers: &[mailparse::MailHeader]) -> Result<String> {
    if let Some(message_id) = headers.get_header_value(HeaderDef::MessageId)? {
        Ok(crate::mimeparser::parse_message_id(&message_id)?)
    } else {
        Err(Error::Other("prefetch: No message ID found".to_string()))
    }
}

fn prefetch_is_reply_to_chat_message(
    context: &Context,
    headers: &[mailparse::MailHeader],
) -> Result<bool> {
    if let Some(value) = headers.get_header_value(HeaderDef::InReplyTo)? {
        if is_msgrmsg_rfc724_mid_in_list(context, &value) {
            return Ok(true);
        }
    }

    if let Some(value) = headers.get_header_value(HeaderDef::References)? {
        if is_msgrmsg_rfc724_mid_in_list(context, &value) {
            return Ok(true);
        }
    }

    Ok(false)
}

fn prefetch_should_download(
    context: &Context,
    headers: &[mailparse::MailHeader],
    show_emails: ShowEmails,
) -> Result<bool> {
    let is_chat_message = headers.get_header_value(HeaderDef::ChatVersion)?.is_some();
    let is_reply_to_chat_message = prefetch_is_reply_to_chat_message(context, &headers)?;

    // Autocrypt Setup Message should be shown even if it is from non-chat client.
    let is_autocrypt_setup_message = headers
        .get_header_value(HeaderDef::AutocryptSetupMessage)?
        .is_some();

    let from_field = headers
        .get_header_value(HeaderDef::From_)?
        .unwrap_or_default();

    let (_contact_id, blocked_contact, origin) = from_field_to_contact_id(context, &from_field)?;
    let accepted_contact = origin.is_known();

    let show = is_autocrypt_setup_message
        || match show_emails {
            ShowEmails::Off => is_chat_message || is_reply_to_chat_message,
            ShowEmails::AcceptedContacts => {
                is_chat_message || is_reply_to_chat_message || accepted_contact
            }
            ShowEmails::All => true,
        };
    let show = show && !blocked_contact;
    Ok(show)
}
