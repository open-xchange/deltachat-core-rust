use std::time::Duration;

use deltachat_derive::{FromSql, ToSql};
use rand::{thread_rng, Rng};

use crate::chat;
use crate::config::Config;
use crate::configure::*;
use crate::constants::*;
use crate::context::Context;
use crate::dc_tools::*;
use crate::error::Error;
use crate::events::Event;
use crate::imap::*;
use crate::imex::*;
use crate::location;
use crate::login_param::LoginParam;
use crate::message::{self, Message, MessageState};
use crate::mimefactory::{vec_contains_lowercase, Loaded, MimeFactory};
use crate::param::*;
use crate::sql;
use crate::types::*;
use crate::x::*;
use crate::coi::deltachat_mode::CoiDeltachatMode;

/// Thread IDs
#[derive(Debug, Display, Copy, Clone, PartialEq, Eq, FromPrimitive, ToPrimitive, FromSql, ToSql)]
#[repr(i32)]
enum Thread {
    Unknown = 0,
    Imap = 100,
    Smtp = 5000,
}

impl Default for Thread {
    fn default() -> Self {
        Thread::Unknown
    }
}

#[derive(Debug, Display, Copy, Clone, PartialEq, Eq, FromPrimitive, ToPrimitive, FromSql, ToSql)]
#[repr(i32)]
pub enum Action {
    Unknown = 0,

    // Jobs in the INBOX-thread, range from DC_IMAP_THREAD..DC_IMAP_THREAD+999
    Housekeeping = 105, // low priority ...
    DeleteMsgOnImap = 110,
    MarkseenMdnOnImap = 120,
    MarkseenMsgOnImap = 130,
    MoveMsg = 200,
    SetMetadata = 300,
    GetMetadata = 310,
    ConfigureImap = 900,
    ImexImap = 910, // ... high priority

    // Jobs in the SMTP-thread, range from DC_SMTP_THREAD..DC_SMTP_THREAD+999
    MaybeSendLocations = 5005, // low priority ...
    MaybeSendLocationsEnded = 5007,
    SendMdnOld = 5010,
    SendMdn = 5011,
    SendMsgToSmtpOld = 5900,
    SendMsgToSmtp = 5901, // ... high priority
}

impl Default for Action {
    fn default() -> Self {
        Action::Unknown
    }
}

impl From<Action> for Thread {
    fn from(action: Action) -> Thread {
        use Action::*;

        match action {
            Unknown => Thread::Unknown,

            Housekeeping => Thread::Imap,
            DeleteMsgOnImap => Thread::Imap,
            MarkseenMdnOnImap => Thread::Imap,
            MarkseenMsgOnImap => Thread::Imap,
            MoveMsg => Thread::Imap,
            SetMetadata => Thread::Imap,
            GetMetadata => Thread::Imap,
            ConfigureImap => Thread::Imap,
            ImexImap => Thread::Imap,

            MaybeSendLocations => Thread::Smtp,
            MaybeSendLocationsEnded => Thread::Smtp,
            SendMdnOld => Thread::Smtp,
            SendMdn => Thread::Smtp,
            SendMsgToSmtpOld => Thread::Smtp,
            SendMsgToSmtp => Thread::Smtp,
        }
    }
}

#[derive(Debug, Clone)]
pub struct Job {
    pub job_id: u32,
    pub action: Action,
    pub foreign_id: u32,
    pub desired_timestamp: i64,
    pub added_timestamp: i64,
    pub tries: i32,
    pub param: Params,
    pub try_again: i32,
    pub pending_error: Option<String>,
}

impl Job {
    fn delete(&self, context: &Context) -> bool {
        context
            .sql
            .execute("DELETE FROM jobs WHERE id=?;", params![self.job_id as i32])
            .is_ok()
    }

    fn update(&self, context: &Context) -> bool {
        sql::execute(
            context,
            &context.sql,
            "UPDATE jobs SET desired_timestamp=?, tries=?, param=? WHERE id=?;",
            params![
                self.desired_timestamp,
                self.tries as i64,
                self.param.to_string(),
                self.job_id as i32,
            ],
        )
        .is_ok()
    }

    #[allow(non_snake_case)]
    fn do_DC_JOB_SEND(&mut self, context: &Context) {
        /* connect to SMTP server, if not yet done */
        if !context.smtp.lock().unwrap().is_connected() {
            let loginparam = LoginParam::from_database(context, "configured_");
            let connected = context.smtp.lock().unwrap().connect(context, &loginparam);

            if !connected {
                self.try_again_later(3i32, None);
                return;
            }
        }

        if let Some(filename) = self.param.get(Param::File) {
            if let Ok(body) = dc_read_file(context, filename) {
                if let Some(recipients) = self.param.get(Param::Recipients) {
                    let recipients_list = recipients
                        .split('\x1e')
                        .filter_map(|addr| match lettre::EmailAddress::new(addr.to_string()) {
                            Ok(addr) => Some(addr),
                            Err(err) => {
                                warn!(context, "invalid recipient: {} {:?}", addr, err);
                                None
                            }
                        })
                        .collect::<Vec<_>>();

                    /* if there is a msg-id and it does not exist in the db, cancel sending.
                    this happends if dc_delete_msgs() was called
                    before the generated mime was sent out */
                    if 0 != self.foreign_id && !message::exists(context, self.foreign_id) {
                        warn!(
                            context,
                            "Not sending Message {} as it was deleted", self.foreign_id
                        );
                        return;
                    };

                    // hold the smtp lock during sending of a job and
                    // its ok/error response processing. Note that if a message
                    // was sent we need to mark it in the database ASAP as we
                    // otherwise might send it twice.
                    let mut sock = context.smtp.lock().unwrap();
                    match sock.send(context, recipients_list, body) {
                        Err(err) => {
                            sock.disconnect();
                            warn!(context, "smtp failed: {}", err);
                            self.try_again_later(-1i32, Some(err.to_string()));
                        }
                        Ok(()) => {
                            dc_delete_file(context, filename);
                            // smtp success, update db ASAP, then delete smtp file
                            if 0 != self.foreign_id {
                                message::update_msg_state(
                                    context,
                                    self.foreign_id,
                                    MessageState::OutDelivered,
                                );
                                let chat_id: i32 = context
                                    .sql
                                    .query_get_value(
                                        context,
                                        "SELECT chat_id FROM msgs WHERE id=?",
                                        params![self.foreign_id as i32],
                                    )
                                    .unwrap_or_default();
                                context.call_cb(Event::MsgDelivered {
                                    chat_id: chat_id as u32,
                                    msg_id: self.foreign_id,
                                });
                            }
                            // now also delete the generated file
                            dc_delete_file(context, filename);
                        }
                    }
                } else {
                    warn!(context, "Missing recipients for job {}", self.job_id,);
                }
            }
        }
    }

    // this value does not increase the number of tries
    fn try_again_later(&mut self, try_again: libc::c_int, pending_error: Option<String>) {
        self.try_again = try_again;
        self.pending_error = pending_error;
    }

    #[allow(non_snake_case)]
    fn do_DC_JOB_MOVE_MSG(&mut self, context: &Context) {
        let inbox = context.inbox.read().unwrap();

        if let Ok(msg) = Message::load_from_db(context, self.foreign_id) {
            if context
                .sql
                .get_raw_config_int(context, "folders_configured")
                .unwrap_or_default()
                < 3
            {
                inbox.configure_folders(context, 0x1i32);
            }
            let dest_folder = context
                .sql
                .get_raw_config(context, "configured_mvbox_folder");

            if let Some(dest_folder) = dest_folder {
                let server_folder = msg.server_folder.as_ref().unwrap();
                let mut dest_uid = 0;

                match inbox.mv(
                    context,
                    server_folder,
                    msg.server_uid,
                    &dest_folder,
                    &mut dest_uid,
                ) {
                    ImapResult::RetryLater => {
                        self.try_again_later(3i32, None);
                    }
                    ImapResult::Success => {
                        message::update_server_uid(
                            context,
                            &msg.rfc724_mid,
                            &dest_folder,
                            dest_uid,
                        );
                    }
                    ImapResult::Failed | ImapResult::AlreadyDone => {}
                }
            }
        }
    }

    #[allow(non_snake_case)]
    fn do_DC_JOB_SET_METADATA(&self, context: &Context) {
        if let Some(meta) = self.param.get_map(Param::Metadata) {
            let meta: Vec<Metadata> = meta.iter().map(|(k, v)| Metadata {
                entry: k.to_string(),
                value: if v.is_empty() { None } else { Some(v.to_string()) },
            }).collect();
            let inbox = context.inbox.read().unwrap();
            match inbox.set_metadata(context, "", &meta) {
                Ok(_) => {
                    context.call_cb(Event::SET_METADATA_DONE,
                                    self.foreign_id as uintptr_t, 0);
                },
                Err(e) => {
                    error!(context, self.foreign_id,
                           "Cannot set metadata: {}", e);
                },
            };
        }
    }

    #[allow(non_snake_case)]
    fn do_DC_JOB_GET_METADATA(&self, context: &Context) {
        let (success, text) = if let Some(path) = self.param.get(Param::Metadata) {
            let inbox = context.inbox.read().unwrap();
            let res = inbox.get_metadata(context, "", &[path], MetadataDepth::Zero, None);
            match res {
                Ok(meta) => {
                    if let Some(meta) = meta.first() {
                        if meta.entry == path {
                            (true, meta.value.clone())
                        } else {
                            (false, Some(format!("Invalid path in GETMETADATA response. Expected: {}, got: {}",
                                                 path, meta.entry)))
                        }
                    } else {
                        (true, None)
                    }
                },
                Err(e) => (false, Some(e.to_string())),
            }
        } else {
            (false, Some("Missing metadata path".into()))
        };
        let text = text.map(|s| std::ffi::CString::new(s).unwrap());
        context.call_cb(
            if success { Event::METADATA } else { Event::ERROR },
            self.foreign_id as uintptr_t,
            text.map(|s| s.as_ptr()).unwrap_or(ptr::null()) as uintptr_t
        );
    }

    #[allow(non_snake_case)]
    fn do_DC_JOB_DELETE_MSG_ON_IMAP(&mut self, context: &Context) {
        let inbox = context.inbox.read().unwrap();

        if let Ok(mut msg) = Message::load_from_db(context, self.foreign_id) {
            if !msg.rfc724_mid.is_empty() {
                /* eg. device messages have no Message-ID */
                if message::rfc724_mid_cnt(context, &msg.rfc724_mid) > 1 {
                    info!(
                        context,
                        "The message is deleted from the server when all parts are deleted.",
                    );
                } else {
                    /* if this is the last existing part of the message,
                    we delete the message from the server */
                    let mid = msg.rfc724_mid;
                    let server_folder = msg.server_folder.as_ref().unwrap();
                    let res = inbox.delete_msg(context, &mid, server_folder, &mut msg.server_uid);
                    if res == ImapResult::RetryLater {
                        self.try_again_later(-1i32, None);
                        return;
                    }
                }
                Message::delete_from_db(context, msg.id);
            }
        }
    }

    #[allow(non_snake_case)]
    fn do_DC_JOB_MARKSEEN_MSG_ON_IMAP(&mut self, context: &Context) {
        let inbox = context.inbox.read().unwrap();

        if let Ok(msg) = Message::load_from_db(context, self.foreign_id) {
            let folder = msg.server_folder.as_ref().unwrap();
            match inbox.set_seen(context, folder, msg.server_uid) {
                ImapResult::RetryLater => {
                    self.try_again_later(3i32, None);
                }
                ImapResult::AlreadyDone => {}
                ImapResult::Success | ImapResult::Failed => {
                    // XXX the message might just have been moved
                    // we want to send out an MDN anyway
                    // The job will not be retried so locally
                    // there is no risk of double-sending MDNs.
                    if 0 != msg.param.get_int(Param::WantsMdn).unwrap_or_default()
                        && context.get_config_bool(Config::MdnsEnabled)
                    {
                        if let Err(err) = send_mdn(context, msg.id) {
                            warn!(context, "could not send out mdn for {}: {}", msg.id, err);
                        }
                    }
                }
            }
        }
    }

    #[allow(non_snake_case)]
    fn do_DC_JOB_MARKSEEN_MDN_ON_IMAP(&mut self, context: &Context) {
        let folder = self
            .param
            .get(Param::ServerFolder)
            .unwrap_or_default()
            .to_string();
        let uid = self.param.get_int(Param::ServerUid).unwrap_or_default() as u32;
        let inbox = context.inbox.read().unwrap();
        if inbox.set_seen(context, &folder, uid) == ImapResult::RetryLater {
            self.try_again_later(3i32, None);
            return;
        }
        if 0 != self.param.get_int(Param::AlsoMove).unwrap_or_default() {
            if context
                .sql
                .get_raw_config_int(context, "folders_configured")
                .unwrap_or_default()
                < 3
            {
                inbox.configure_folders(context, 0x1i32);
            }
            let dest_folder = context
                .sql
                .get_raw_config(context, "configured_mvbox_folder");
            if let Some(dest_folder) = dest_folder {
                let mut dest_uid = 0;
                if ImapResult::RetryLater
                    == inbox.mv(context, &folder, uid, &dest_folder, &mut dest_uid)
                {
                    self.try_again_later(3, None);
                }
            }
        }
    }
}

/* delete all pending jobs with the given action */
pub fn job_kill_action(context: &Context, action: Action) -> bool {
    sql::execute(
        context,
        &context.sql,
        "DELETE FROM jobs WHERE action=?;",
        params![action],
    )
    .is_ok()
}

pub fn perform_imap_fetch(context: &Context) {
    let inbox = context.inbox.read().unwrap();
    let start = std::time::Instant::now();

    if 0 == connect_to_inbox(context, &inbox) {
        return;
    }
    if !context.get_config_bool(Config::InboxWatch) {
        info!(context, "INBOX-watch disabled.",);
        return;
    }
    info!(context, "INBOX-fetch started...",);
    inbox.fetch(context);
    if inbox.should_reconnect() {
        info!(context, "INBOX-fetch aborted, starting over...",);
        inbox.fetch(context);
    }
    info!(
        context,
        "INBOX-fetch done in {:.4} ms.",
        start.elapsed().as_nanos() as f64 / 1000.0,
    );
}

pub fn perform_imap_idle(context: &Context) {
    let inbox = context.inbox.read().unwrap();

    connect_to_inbox(context, &inbox);

    if *context.perform_inbox_jobs_needed.clone().read().unwrap() {
        info!(
            context,
            "INBOX-IDLE will not be started because of waiting jobs."
        );
        return;
    }
    info!(context, "INBOX-IDLE started...");
    inbox.idle(context);
    info!(context, "INBOX-IDLE ended.");
}

pub fn perform_mvbox_fetch(context: &Context) {
    let use_network = context.get_config_bool(Config::MvboxWatch);
    context
        .mvbox_thread
        .write()
        .unwrap()
        .fetch(context, use_network);
}

pub fn perform_mvbox_idle(context: &Context) {
    let use_network = context.get_config_bool(Config::MvboxWatch);

    context
        .mvbox_thread
        .read()
        .unwrap()
        .idle(context, use_network);
}

pub fn interrupt_mvbox_idle(context: &Context) {
    context.mvbox_thread.read().unwrap().interrupt_idle(context);
}

pub fn perform_sentbox_fetch(context: &Context) {
    let use_network = context.get_config_bool(Config::SentboxWatch);

    context
        .sentbox_thread
        .write()
        .unwrap()
        .fetch(context, use_network);
}

pub fn perform_sentbox_idle(context: &Context) {
    let use_network = context.get_config_bool(Config::SentboxWatch);

    context
        .sentbox_thread
        .read()
        .unwrap()
        .idle(context, use_network);
}

pub fn interrupt_sentbox_idle(context: &Context) {
    context
        .sentbox_thread
        .read()
        .unwrap()
        .interrupt_idle(context);
}

pub fn perform_smtp_jobs(context: &Context) {
    let probe_smtp_network = {
        let &(ref lock, _) = &*context.smtp_state.clone();
        let mut state = lock.lock().unwrap();

        let probe_smtp_network = state.probe_network;
        state.probe_network = false;
        state.perform_jobs_needed = 0;

        if state.suspended {
            info!(context, "SMTP-jobs suspended.",);
            return;
        }
        state.doing_jobs = true;
        probe_smtp_network
    };

    info!(context, "SMTP-jobs started...",);
    job_perform(context, Thread::Smtp, probe_smtp_network);
    info!(context, "SMTP-jobs ended.");

    {
        let &(ref lock, _) = &*context.smtp_state.clone();
        let mut state = lock.lock().unwrap();

        state.doing_jobs = false;
    }
}

pub fn perform_smtp_idle(context: &Context) {
    info!(context, "SMTP-idle started...",);
    {
        let &(ref lock, ref cvar) = &*context.smtp_state.clone();
        let mut state = lock.lock().unwrap();

        if state.perform_jobs_needed == 1 {
            info!(
                context,
                "SMTP-idle will not be started because of waiting jobs.",
            );
        } else {
            let dur = get_next_wakeup_time(context, Thread::Smtp);

            loop {
                let res = cvar.wait_timeout(state, dur).unwrap();
                state = res.0;

                if state.idle == true || res.1.timed_out() {
                    // We received the notification and the value has been updated, we can leave.
                    break;
                }
            }
            state.idle = false;
        }
    }

    info!(context, "SMTP-idle ended.",);
}

fn get_next_wakeup_time(context: &Context, thread: Thread) -> Duration {
    let t: i64 = context
        .sql
        .query_get_value(
            context,
            "SELECT MIN(desired_timestamp) FROM jobs WHERE thread=?;",
            params![thread],
        )
        .unwrap_or_default();

    let mut wakeup_time = Duration::new(10 * 60, 0);
    let now = time();
    if t > 0 {
        if t > now {
            wakeup_time = Duration::new((t - now) as u64, 0);
        } else {
            wakeup_time = Duration::new(0, 0);
        }
    }

    wakeup_time
}

pub fn maybe_network(context: &Context) {
    {
        let &(ref lock, _) = &*context.smtp_state.clone();
        let mut state = lock.lock().unwrap();
        state.probe_network = true;

        *context.probe_imap_network.write().unwrap() = true;
    }

    interrupt_smtp_idle(context);
    interrupt_imap_idle(context);
    interrupt_mvbox_idle(context);
    interrupt_sentbox_idle(context);
}

pub fn job_action_exists(context: &Context, action: Action) -> bool {
    context
        .sql
        .exists("SELECT id FROM jobs WHERE action=?;", params![action])
        .unwrap_or_default()
}

/* special case for DC_JOB_SEND_MSG_TO_SMTP */
#[allow(non_snake_case)]
pub fn job_send_msg(context: &Context, msg_id: u32) -> Result<(), Error> {
    let mut mimefactory = MimeFactory::load_msg(context, msg_id)?;

    if chat::msgtype_has_file(mimefactory.msg.type_0) {
        let file_param = mimefactory
            .msg
            .param
            .get(Param::File)
            .map(|s| s.to_string());
        if let Some(pathNfilename) = file_param {
            if (mimefactory.msg.type_0 == Viewtype::Image
                || mimefactory.msg.type_0 == Viewtype::Gif)
                && !mimefactory.msg.param.exists(Param::Width)
            {
                mimefactory.msg.param.set_int(Param::Width, 0);
                mimefactory.msg.param.set_int(Param::Height, 0);

                if let Ok(buf) = dc_read_file(context, pathNfilename) {
                    if let Ok((width, height)) = dc_get_filemeta(&buf) {
                        mimefactory.msg.param.set_int(Param::Width, width as i32);
                        mimefactory.msg.param.set_int(Param::Height, height as i32);
                    }
                }
                mimefactory.msg.save_param_to_disk(context);
            }
        }
    }

    /* create message */
    if let Err(msg) = unsafe { mimefactory.render() } {
        let e = msg.to_string();
        message::set_msg_failed(context, msg_id, Some(e));
        return Err(msg);
    }
    if 0 != mimefactory
        .msg
        .param
        .get_int(Param::GuranteeE2ee)
        .unwrap_or_default()
        && !mimefactory.out_encrypted
    {
        /* unrecoverable */
        message::set_msg_failed(
            context,
            msg_id,
            Some("End-to-end-encryption unavailable unexpectedly."),
        );
        bail!(
            "e2e encryption unavailable {} - {:?}",
            msg_id,
            mimefactory.msg.param.get_int(Param::GuranteeE2ee),
        );
    }
    if context.get_config_bool(Config::BccSelf)
        && !vec_contains_lowercase(&mimefactory.recipients_addr, &mimefactory.from_addr)
    {
        mimefactory.recipients_names.push("".to_string());
        mimefactory
            .recipients_addr
            .push(mimefactory.from_addr.to_string());
    }

    if mimefactory.recipients_addr.is_empty() {
        warn!(
            context,
            "message {} has no recipient, skipping smtp-send", msg_id
        );
        return Ok(());
    }

    if mimefactory.out_gossiped {
        chat::set_gossiped_timestamp(context, mimefactory.msg.chat_id, time());
    }
    if 0 != mimefactory.out_last_added_location_id {
        if let Err(err) = location::set_kml_sent_timestamp(context, mimefactory.msg.chat_id, time())
        {
            error!(context, "Failed to set kml sent_timestamp: {:?}", err);
        }
        if !mimefactory.msg.hidden {
            if let Err(err) = location::set_msg_location_id(
                context,
                mimefactory.msg.id,
                mimefactory.out_last_added_location_id,
            ) {
                error!(context, "Failed to set msg_location_id: {:?}", err);
            }
        }
    }
    if mimefactory.out_encrypted
        && mimefactory
            .msg
            .param
            .get_int(Param::GuranteeE2ee)
            .unwrap_or_default()
            == 0
    {
        mimefactory.msg.param.set_int(Param::GuranteeE2ee, 1);
        mimefactory.msg.save_param_to_disk(context);
    }
    add_smtp_job(context, Action::SendMsgToSmtp, &mut mimefactory)?;

    Ok(())
}

pub fn perform_imap_jobs(context: &Context) {
    info!(context, "dc_perform_imap_jobs starting.",);

    let probe_imap_network = *context.probe_imap_network.clone().read().unwrap();
    *context.probe_imap_network.write().unwrap() = false;
    *context.perform_inbox_jobs_needed.write().unwrap() = false;

    job_perform(context, Thread::Imap, probe_imap_network);
    info!(context, "dc_perform_imap_jobs ended.",);
}

pub fn perform_mvbox_jobs(context: &Context) {
    info!(context, "dc_perform_mbox_jobs EMPTY (for now).",);
}

pub fn perform_sentbox_jobs(context: &Context) {
    info!(context, "dc_perform_sentbox_jobs EMPTY (for now).",);
}

fn job_perform(context: &Context, thread: Thread, probe_network: bool) {
    let query = if !probe_network {
        // processing for first-try and after backoff-timeouts:
        // process jobs in the order they were added.
        "SELECT id, action, foreign_id, param, added_timestamp, desired_timestamp, tries \
         FROM jobs WHERE thread=? AND desired_timestamp<=? ORDER BY action DESC, added_timestamp;"
    } else {
        // processing after call to dc_maybe_network():
        // process _all_ pending jobs that failed before
        // in the order of their backoff-times.
        "SELECT id, action, foreign_id, param, added_timestamp, desired_timestamp, tries \
         FROM jobs WHERE thread=? AND tries>0 ORDER BY desired_timestamp, action DESC;"
    };

    let params_no_probe = params![thread as i64, time()];
    let params_probe = params![thread as i64];
    let params: &[&dyn rusqlite::ToSql] = if !probe_network {
        params_no_probe
    } else {
        params_probe
    };

    let jobs: Result<Vec<Job>, _> = context.sql.query_map(
        query,
        params,
        |row| {
            let job = Job {
                job_id: row.get(0)?,
                action: row.get(1)?,
                foreign_id: row.get(2)?,
                desired_timestamp: row.get(5)?,
                added_timestamp: row.get(4)?,
                tries: row.get(6)?,
                param: row.get::<_, String>(3)?.parse().unwrap_or_default(),
                try_again: 0,
                pending_error: None,
            };

            Ok(job)
        },
        |jobs| jobs.collect::<Result<Vec<Job>, _>>().map_err(Into::into),
    );
    match jobs {
        Ok(ref _res) => {}
        Err(ref err) => {
            info!(context, "query failed: {:?}", err);
        }
    }

    for mut job in jobs.unwrap_or_default() {
        info!(
            context,
            "{}-job #{}, action {} started...",
            if thread == Thread::Imap {
                "INBOX"
            } else {
                "SMTP"
            },
            job.job_id,
            job.action,
        );

        // some configuration jobs are "exclusive":
        // - they are always executed in the imap-thread and the smtp-thread is suspended during execution
        // - they may change the database handle change the database handle; we do not keep old pointers therefore
        // - they can be re-executed one time AT_ONCE, but they are not save in the database for later execution
        if Action::ConfigureImap == job.action || Action::ImexImap == job.action {
            job_kill_action(context, job.action);
            context
                .sentbox_thread
                .clone()
                .read()
                .unwrap()
                .suspend(context);
            context
                .mvbox_thread
                .clone()
                .read()
                .unwrap()
                .suspend(context);
            suspend_smtp_thread(context, true);
        }

        let mut tries = 0;
        while tries <= 1 {
            // this can be modified by a job using dc_job_try_again_later()
            job.try_again = 0;

            match job.action {
                Action::Unknown => {
                    warn!(context, "Unknown job id found");
                }
                Action::SendMsgToSmtp => job.do_DC_JOB_SEND(context),
                Action::DeleteMsgOnImap => job.do_DC_JOB_DELETE_MSG_ON_IMAP(context),
                Action::MarkseenMsgOnImap => job.do_DC_JOB_MARKSEEN_MSG_ON_IMAP(context),
                Action::MarkseenMdnOnImap => job.do_DC_JOB_MARKSEEN_MDN_ON_IMAP(context),
                Action::MoveMsg => job.do_DC_JOB_MOVE_MSG(context),
                Action::SetMetadata => job.do_DC_JOB_SET_METADATA(context),
                Action::GetMetadata => job.do_DC_JOB_GET_METADATA(context),
                Action::SendMdn => job.do_DC_JOB_SEND(context),
                Action::ConfigureImap => dc_job_do_DC_JOB_CONFIGURE_IMAP(context),
                Action::ImexImap => match job_do_DC_JOB_IMEX_IMAP(context, &job) {
                    Ok(()) => {}
                    Err(err) => {
                        error!(context, "{}", err);
                    }
                },
                Action::MaybeSendLocations => {
                    location::job_do_DC_JOB_MAYBE_SEND_LOCATIONS(context, &job)
                }
                Action::MaybeSendLocationsEnded => {
                    location::job_do_DC_JOB_MAYBE_SEND_LOC_ENDED(context, &mut job)
                }
                Action::Housekeeping => sql::housekeeping(context),
                Action::SendMdnOld => {}
                Action::SendMsgToSmtpOld => {}
            }
            if job.try_again != -1 {
                break;
            }
            tries += 1
        }
        if Action::ConfigureImap == job.action || Action::ImexImap == job.action {
            context
                .sentbox_thread
                .clone()
                .read()
                .unwrap()
                .unsuspend(context);
            context
                .mvbox_thread
                .clone()
                .read()
                .unwrap()
                .unsuspend(context);
            suspend_smtp_thread(context, false);
            break;
        } else if job.try_again == 2 {
            // just try over next loop unconditionally, the ui typically interrupts idle when the file (video) is ready
            info!(
                context,
                "{}-job #{} not yet ready and will be delayed.",
                if thread == Thread::Imap {
                    "INBOX"
                } else {
                    "SMTP"
                },
                job.job_id
            );
        } else if job.try_again == -1 || job.try_again == 3 {
            let tries = job.tries + 1;
            if tries < 17 {
                job.tries = tries;
                let time_offset = get_backoff_time_offset(tries);
                job.desired_timestamp = job.added_timestamp + time_offset;
                job.update(context);
                info!(
                    context,
                    "{}-job #{} not succeeded on try #{}, retry in ADD_TIME+{} (in {} seconds).",
                    if thread == Thread::Imap {
                        "INBOX"
                    } else {
                        "SMTP"
                    },
                    job.job_id as u32,
                    tries,
                    time_offset,
                    job.added_timestamp + time_offset - time()
                );
                if thread == Thread::Smtp && tries < 17 - 1 {
                    context
                        .smtp_state
                        .clone()
                        .0
                        .lock()
                        .unwrap()
                        .perform_jobs_needed = 2;
                }
            } else {
                if job.action == Action::SendMsgToSmtp {
                    message::set_msg_failed(context, job.foreign_id, job.pending_error.as_ref());
                }
                job.delete(context);
            }
            if !probe_network {
                continue;
            }
            // on dc_maybe_network() we stop trying here;
            // these jobs are already tried once.
            // otherwise, we just continue with the next job
            // to give other jobs a chance being tried at least once.
            break;
        } else {
            job.delete(context);
        }
    }
}

#[allow(non_snake_case)]
fn get_backoff_time_offset(c_tries: libc::c_int) -> i64 {
    // results in ~3 weeks for the last backoff timespan
    let N = 2_i32.pow((c_tries - 1) as u32) * 60;
    let mut rng = thread_rng();
    let n: i32 = rng.gen();
    let mut seconds = n % (N + 1);
    if seconds < 1 {
        seconds = 1;
    }
    seconds as i64
}

fn suspend_smtp_thread(context: &Context, suspend: bool) {
    context.smtp_state.0.lock().unwrap().suspended = suspend;
    if suspend {
        loop {
            if !context.smtp_state.0.lock().unwrap().doing_jobs {
                return;
            }
            std::thread::sleep(std::time::Duration::from_micros(300 * 1000));
        }
    }
}

pub fn connect_to_inbox(context: &Context, inbox: &Imap) -> libc::c_int {
    let ret_connected = dc_connect_to_configured_imap(context, inbox);
    if 0 != ret_connected {

        let coi_deltachat_mode =
            context
                .get_coi_config()
                .map(|config| config.get_coi_deltachat_mode())
                .unwrap_or(CoiDeltachatMode::Disabled);

        inbox.set_watch_folder(coi_deltachat_mode.get_inbox_folder_override().unwrap_or("INBOX").into());
        context.set_coi_deltachat_mode(coi_deltachat_mode);
    }
    ret_connected
}

fn send_mdn(context: &Context, msg_id: u32) -> Result<(), Error> {
    let mut mimefactory = MimeFactory::load_mdn(context, msg_id)?;
    unsafe { mimefactory.render()? };
    add_smtp_job(context, Action::SendMdn, &mut mimefactory)?;

    Ok(())
}

#[allow(non_snake_case)]
fn add_smtp_job(context: &Context, action: Action, mimefactory: &MimeFactory) -> Result<(), Error> {
    ensure!(
        !mimefactory.recipients_addr.is_empty(),
        "no recipients for smtp job set"
    );
    let mut param = Params::new();
    let bytes = unsafe {
        std::slice::from_raw_parts(
            (*mimefactory.out).str_0 as *const u8,
            (*mimefactory.out).len,
        )
    };
    let bpath = context.new_blob_file(&mimefactory.rfc724_mid, bytes)?;
    let recipients = mimefactory.recipients_addr.join("\x1e");
    param.set(Param::File, &bpath);
    param.set(Param::Recipients, &recipients);
    job_add(
        context,
        action,
        (if mimefactory.loaded == Loaded::Message {
            mimefactory.msg.id
        } else {
            0
        }) as libc::c_int,
        param,
        0,
    );

    Ok(())
}

pub fn job_add(
    context: &Context,
    action: Action,
    foreign_id: libc::c_int,
    param: Params,
    delay_seconds: i64,
) {
    if action == Action::Unknown {
        error!(context, "Invalid action passed to job_add");
        return;
    }

    let timestamp = time();
    let thread: Thread = action.into();

    sql::execute(
        context,
        &context.sql,
        "INSERT INTO jobs (added_timestamp, thread, action, foreign_id, param, desired_timestamp) VALUES (?,?,?,?,?,?);",
        params![
            timestamp,
            thread,
            action,
            foreign_id,
            param.to_string(),
            (timestamp + delay_seconds as i64)
        ]
    ).ok();

    match thread {
        Thread::Imap => interrupt_imap_idle(context),
        Thread::Smtp => interrupt_smtp_idle(context),
        Thread::Unknown => {}
    }
}

pub fn interrupt_smtp_idle(context: &Context) {
    info!(context, "Interrupting SMTP-idle...",);

    let &(ref lock, ref cvar) = &*context.smtp_state.clone();
    let mut state = lock.lock().unwrap();

    state.perform_jobs_needed = 1;
    state.idle = true;
    cvar.notify_one();
}

pub fn interrupt_imap_idle(context: &Context) {
    info!(context, "Interrupting IMAP-IDLE...",);

    *context.perform_inbox_jobs_needed.write().unwrap() = true;
    context.inbox.read().unwrap().interrupt_idle();
}
