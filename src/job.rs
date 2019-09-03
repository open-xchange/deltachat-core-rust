use std::ffi::CStr;
use std::ptr;
use std::time::Duration;

use deltachat_derive::{FromSql, ToSql};
use rand::{thread_rng, Rng};

use crate::chat;
use crate::coi_message_filter::CoiMessageFilter;
use crate::configure::*;
use crate::constants::*;
use crate::context::Context;
use crate::dc_imex::*;
use crate::dc_location::*;
use crate::dc_loginparam::*;
use crate::dc_mimefactory::*;
use crate::dc_tools::*;
use crate::imap::*;
use crate::message::*;
use crate::param::*;
use crate::sql;
use crate::types::*;
use crate::x::*;

/// Thread IDs
#[derive(Debug, Display, Copy, Clone, PartialEq, Eq, FromPrimitive, ToPrimitive, FromSql, ToSql)]
#[repr(i32)]
enum Thread {
    Imap = 100,
    Smtp = 5000,
}

#[derive(Debug, Display, Copy, Clone, PartialEq, Eq, FromPrimitive, ToPrimitive, FromSql, ToSql)]
#[repr(i32)]
pub enum Action {
    // Jobs in the INBOX-thread, range from DC_IMAP_THREAD..DC_IMAP_THREAD+999
    Housekeeping = 105, // low priority ...
    DeleteMsgOnImap = 110,
    MarkseenMdnOnImap = 120,
    MarkseenMsgOnImap = 130,
    MoveMsg = 200,
    SetMetadata = 300,
    GetWebPushSubscription = 310,
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

impl From<Action> for Thread {
    fn from(action: Action) -> Thread {
        use Action::*;

        match action {
            Housekeeping => Thread::Imap,
            DeleteMsgOnImap => Thread::Imap,
            MarkseenMdnOnImap => Thread::Imap,
            MarkseenMsgOnImap => Thread::Imap,
            MoveMsg => Thread::Imap,
            SetMetadata => Thread::Imap,
            GetWebPushSubscription => Thread::Imap,
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
        let ok_to_continue;
        let mut filename = ptr::null_mut();
        let mut buf = ptr::null_mut();
        let mut buf_bytes = 0;

        /* connect to SMTP server, if not yet done */
        if !context.smtp.lock().unwrap().is_connected() {
            let loginparam = dc_loginparam_read(context, &context.sql, "configured_");
            let connected = context.smtp.lock().unwrap().connect(context, &loginparam);

            if !connected {
                self.try_again_later(3i32, None);
                ok_to_continue = false;
            } else {
                ok_to_continue = true;
            }
        } else {
            ok_to_continue = true;
        }
        if ok_to_continue {
            let filename_s = self.param.get(Param::File).unwrap_or_default();
            filename = unsafe { filename_s.strdup() };
            if unsafe { strlen(filename) } == 0 {
                warn!(context, 0, "Missing file name for job {}", self.job_id,);
            } else if 0 != unsafe { dc_read_file(context, filename, &mut buf, &mut buf_bytes) } {
                let recipients = self.param.get(Param::Recipients);
                if recipients.is_none() {
                    warn!(context, 0, "Missing recipients for job {}", self.job_id,);
                } else {
                    let recipients_list = recipients
                        .unwrap()
                        .split("\x1e")
                        .filter_map(|addr| match lettre::EmailAddress::new(addr.to_string()) {
                            Ok(addr) => Some(addr),
                            Err(err) => {
                                eprintln!("WARNING: invalid recipient: {} {:?}", addr, err);
                                None
                            }
                        })
                        .collect::<Vec<_>>();
                    /* if there is a msg-id and it does not exist in the db, cancel sending.
                    this happends if dc_delete_msgs() was called
                    before the generated mime was sent out */
                    let ok_to_continue1;
                    if 0 != self.foreign_id {
                        if 0 == unsafe { dc_msg_exists(context, self.foreign_id) } {
                            warn!(
                                context,
                                0,
                                "Message {} for job {} does not exist",
                                self.foreign_id,
                                self.job_id,
                            );
                            ok_to_continue1 = false;
                        } else {
                            ok_to_continue1 = true;
                        }
                    } else {
                        ok_to_continue1 = true;
                    }
                    if ok_to_continue1 {
                        /* send message */
                        let body = unsafe {
                            std::slice::from_raw_parts(buf as *const u8, buf_bytes).to_vec()
                        };

                        // hold the smtp lock during sending of a job and
                        // its ok/error response processing. Note that if a message
                        // was sent we need to mark it in the database as we
                        // otherwise might send it twice.
                        let mut sock = context.smtp.lock().unwrap();
                        if 0 == sock.send(context, recipients_list, body) {
                            sock.disconnect();
                            self.try_again_later(-1i32, sock.error.clone());
                        } else {
                            dc_delete_file(context, filename_s);
                            if 0 != self.foreign_id {
                                dc_update_msg_state(
                                    context,
                                    self.foreign_id,
                                    MessageState::OutDelivered,
                                );
                                let chat_id: i32 = context
                                    .sql
                                    .query_row_col(
                                        context,
                                        "SELECT chat_id FROM msgs WHERE id=?",
                                        params![self.foreign_id as i32],
                                        0,
                                    )
                                    .unwrap_or_default();
                                context.call_cb(
                                    Event::MSG_DELIVERED,
                                    chat_id as uintptr_t,
                                    self.foreign_id as uintptr_t,
                                );
                            }
                        }
                    }
                }
            }
        }
        unsafe { free(buf) };
        unsafe { free(filename.cast()) };
    }

    // this value does not increase the number of tries
    fn try_again_later(&mut self, try_again: libc::c_int, pending_error: Option<String>) {
        self.try_again = try_again;
        self.pending_error = pending_error;
    }

    #[allow(non_snake_case)]
    fn do_DC_JOB_MOVE_MSG(&mut self, context: &Context) {
        let ok_to_continue;
        let mut dest_uid = 0;

        let inbox = context.inbox.read().unwrap();

        if !inbox.is_connected() {
            connect_to_inbox(context, &inbox);
            if !inbox.is_connected() {
                self.try_again_later(3, None);
                ok_to_continue = false;
            } else {
                ok_to_continue = true;
            }
        } else {
            ok_to_continue = true;
        }
        if ok_to_continue {
            if let Ok(msg) = dc_msg_load_from_db(context, self.foreign_id) {
                if context
                    .sql
                    .get_config_int(context, "folders_configured")
                    .unwrap_or_default()
                    < 3
                {
                    inbox.configure_folders(context, 0x1i32);
                }
                let dest_folder = context.sql.get_config(context, "configured_mvbox_folder");

                if let Some(dest_folder) = dest_folder {
                    let server_folder = msg.server_folder.as_ref().unwrap();

                    match inbox.mv(
                        context,
                        server_folder,
                        msg.server_uid,
                        &dest_folder,
                        &mut dest_uid,
                    ) as libc::c_uint
                    {
                        1 => {
                            self.try_again_later(3i32, None);
                        }
                        3 => {
                            dc_update_server_uid(context, msg.rfc724_mid, &dest_folder, dest_uid);
                        }
                        0 | 2 | _ => {}
                    }
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
                Ok(_) => context.call_cb(Event::SET_METADATA_DONE,
                                         self.foreign_id as uintptr_t, 0),
                Err(e) => context.call_cb(Event::ERROR,
                                          self.foreign_id as uintptr_t,
                                          e.to_string().as_ptr() as uintptr_t),
            };
        }
    }

    #[allow(non_snake_case)]
    fn do_DC_JOB_GET_WEBPUSH_SUBSCRIPTION(&self, context: &Context) {
        let (success, text) = if let Some(uid) = self.param.get(Param::Metadata) {
            let inbox = context.inbox.read().unwrap();
            let key = [crate::webpush::SUBSCRIPTIONS, uid].concat();
            let res = inbox.get_metadata(context, "", &[&key], MetadataDepth::Zero, None);
            match res {
                Ok(meta) => {
                    if let Some(meta) = meta.first() {
                        if meta.entry == key {
                            (true, meta.value.clone())
                        } else {
                            (false, Some(format!("Invalid path in GETMETADATA response. expected: {}, got: {}",
                                                 key, meta.entry)))
                        }
                    } else {
                        (true, None)
                    }
                },
                Err(e) => (false, Some(e.to_string())),
            }
        } else {
            (false, Some("Missing subscription ID".into()))
        };
        context.call_cb(
            if success { Event::WEBPUSH_SUBSCRIPTION } else { Event::ERROR },
            self.foreign_id as uintptr_t,
            text.map(|s| s.as_ptr()).unwrap_or(ptr::null()) as uintptr_t
        );
    }

    #[allow(non_snake_case)]
    fn do_DC_JOB_DELETE_MSG_ON_IMAP(&mut self, context: &Context) {
        let mut delete_from_server = 1;
        let inbox = context.inbox.read().unwrap();

        if let Ok(mut msg) = dc_msg_load_from_db(context, self.foreign_id) {
            if !(msg.rfc724_mid.is_null()
                || unsafe { *msg.rfc724_mid.offset(0isize) as libc::c_int == 0 })
            {
                let ok_to_continue1;
                /* eg. device messages have no Message-ID */
                if dc_rfc724_mid_cnt(context, msg.rfc724_mid) != 1 {
                    info!(
                        context,
                        0, "The message is deleted from the server when all parts are deleted.",
                    );
                    delete_from_server = 0i32
                }
                /* if this is the last existing part of the message, we delete the message from the server */
                if 0 != delete_from_server {
                    let ok_to_continue;
                    if !inbox.is_connected() {
                        connect_to_inbox(context, &inbox);
                        if !inbox.is_connected() {
                            self.try_again_later(3i32, None);
                            ok_to_continue = false;
                        } else {
                            ok_to_continue = true;
                        }
                    } else {
                        ok_to_continue = true;
                    }
                    if ok_to_continue {
                        let mid = unsafe { CStr::from_ptr(msg.rfc724_mid).to_str().unwrap() };
                        let server_folder = msg.server_folder.as_ref().unwrap();
                        if 0 == inbox.delete_msg(context, mid, server_folder, &mut msg.server_uid) {
                            self.try_again_later(-1i32, None);
                            ok_to_continue1 = false;
                        } else {
                            ok_to_continue1 = true;
                        }
                    } else {
                        ok_to_continue1 = false;
                    }
                } else {
                    ok_to_continue1 = true;
                }
                if ok_to_continue1 {
                    dc_delete_msg_from_db(context, msg.id);
                }
            }
        }
    }

    #[allow(non_snake_case)]
    fn do_DC_JOB_MARKSEEN_MSG_ON_IMAP(&mut self, context: &Context) {
        let ok_to_continue;
        let inbox = context.inbox.read().unwrap();

        if !inbox.is_connected() {
            connect_to_inbox(context, &inbox);
            if !inbox.is_connected() {
                self.try_again_later(3i32, None);
                ok_to_continue = false;
            } else {
                ok_to_continue = true;
            }
        } else {
            ok_to_continue = true;
        }
        if ok_to_continue {
            if let Ok(msg) = dc_msg_load_from_db(context, self.foreign_id) {
                let server_folder = msg.server_folder.as_ref().unwrap();
                match inbox.set_seen(context, server_folder, msg.server_uid) as libc::c_uint {
                    0 => {}
                    1 => {
                        self.try_again_later(3i32, None);
                    }
                    _ => {
                        if 0 != msg.param.get_int(Param::WantsMdn).unwrap_or_default()
                            && 0 != context
                                .sql
                                .get_config_int(context, "mdns_enabled")
                                .unwrap_or_else(|| 1)
                        {
                            let folder = msg.server_folder.as_ref().unwrap();

                            match inbox.set_mdnsent(context, folder, msg.server_uid) as libc::c_uint
                            {
                                1 => {
                                    self.try_again_later(3i32, None);
                                }
                                3 => {
                                    send_mdn(context, msg.id);
                                }
                                0 | 2 | _ => {}
                            }
                        }
                    }
                }
            }
        }
    }

    #[allow(non_snake_case)]
    fn do_DC_JOB_MARKSEEN_MDN_ON_IMAP(&mut self, context: &Context) {
        let ok_to_continue;
        let folder = self
            .param
            .get(Param::ServerFolder)
            .unwrap_or_default()
            .to_string();
        let uid = self.param.get_int(Param::ServerUid).unwrap_or_default() as u32;
        let mut dest_uid = 0;
        let inbox = context.inbox.read().unwrap();

        if !inbox.is_connected() {
            connect_to_inbox(context, &inbox);
            if !inbox.is_connected() {
                self.try_again_later(3, None);
                ok_to_continue = false;
            } else {
                ok_to_continue = true;
            }
        } else {
            ok_to_continue = true;
        }
        if ok_to_continue {
            if inbox.set_seen(context, &folder, uid) == 0 {
                self.try_again_later(3i32, None);
            }
            if 0 != self.param.get_int(Param::AlsoMove).unwrap_or_default() {
                if context
                    .sql
                    .get_config_int(context, "folders_configured")
                    .unwrap_or_default()
                    < 3
                {
                    inbox.configure_folders(context, 0x1i32);
                }
                let dest_folder = context.sql.get_config(context, "configured_mvbox_folder");
                if let Some(dest_folder) = dest_folder {
                    if 1 == inbox.mv(context, folder, uid, dest_folder, &mut dest_uid)
                        as libc::c_uint
                    {
                        self.try_again_later(3, None);
                    }
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
    if context
        .sql
        .get_config_int(context, "inbox_watch")
        .unwrap_or_else(|| 1)
        == 0
    {
        info!(context, 0, "INBOX-watch disabled.",);
        return;
    }
    info!(context, 0, "INBOX-fetch started...",);
    inbox.fetch(context);
    if inbox.should_reconnect() {
        info!(context, 0, "INBOX-fetch aborted, starting over...",);
        inbox.fetch(context);
    }
    info!(
        context,
        0,
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
            0, "INBOX-IDLE will not be started because of waiting jobs."
        );
        return;
    }
    info!(context, 0, "INBOX-IDLE started...");
    inbox.idle(context);
    info!(context, 0, "INBOX-IDLE ended.");
}

pub fn perform_mvbox_fetch(context: &Context) {
    let use_network = context
        .sql
        .get_config_int(context, "mvbox_watch")
        .unwrap_or_else(|| 1);

    context
        .mvbox_thread
        .write()
        .unwrap()
        .fetch(context, use_network == 1);
}

pub fn perform_mvbox_idle(context: &Context) {
    let use_network = context
        .sql
        .get_config_int(context, "mvbox_watch")
        .unwrap_or_else(|| 1);

    context
        .mvbox_thread
        .read()
        .unwrap()
        .idle(context, use_network == 1);
}

pub fn interrupt_mvbox_idle(context: &Context) {
    context.mvbox_thread.read().unwrap().interrupt_idle(context);
}

pub fn perform_sentbox_fetch(context: &Context) {
    let use_network = context
        .sql
        .get_config_int(context, "sentbox_watch")
        .unwrap_or_else(|| 1);

    context
        .sentbox_thread
        .write()
        .unwrap()
        .fetch(context, use_network == 1);
}

pub fn perform_sentbox_idle(context: &Context) {
    let use_network = context
        .sql
        .get_config_int(context, "sentbox_watch")
        .unwrap_or_else(|| 1);

    context
        .sentbox_thread
        .read()
        .unwrap()
        .idle(context, use_network == 1);
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
            info!(context, 0, "SMTP-jobs suspended.",);
            return;
        }
        state.doing_jobs = true;
        probe_smtp_network
    };

    info!(context, 0, "SMTP-jobs started...",);
    job_perform(context, Thread::Smtp, probe_smtp_network);
    info!(context, 0, "SMTP-jobs ended.");

    {
        let &(ref lock, _) = &*context.smtp_state.clone();
        let mut state = lock.lock().unwrap();

        state.doing_jobs = false;
    }
}

pub fn perform_smtp_idle(context: &Context) {
    info!(context, 0, "SMTP-idle started...",);
    {
        let &(ref lock, ref cvar) = &*context.smtp_state.clone();
        let mut state = lock.lock().unwrap();

        if state.perform_jobs_needed == 1 {
            info!(
                context,
                0, "SMTP-idle will not be started because of waiting jobs.",
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

    info!(context, 0, "SMTP-idle ended.",);
}

fn get_next_wakeup_time(context: &Context, thread: Thread) -> Duration {
    let t: i64 = context
        .sql
        .query_row_col(
            context,
            "SELECT MIN(desired_timestamp) FROM jobs WHERE thread=?;",
            params![thread],
            0,
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
pub unsafe fn job_send_msg(context: &Context, msg_id: uint32_t) -> libc::c_int {
    let mut success = 0;

    /* load message data */
    let mimefactory = dc_mimefactory_load_msg(context, msg_id);
    if mimefactory.is_err() || mimefactory.as_ref().unwrap().from_addr.is_null() {
        warn!(
            context,
            0, "Cannot load data to send, maybe the message is deleted in between.",
        );
    } else {
        let mut mimefactory = mimefactory.unwrap();
        // no redo, no IMAP. moreover, as the data does not exist, there is no need in calling dc_set_msg_failed()
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

                    if let Some(buf) = dc_read_file_safe(context, pathNfilename) {
                        if let Ok((width, height)) = dc_get_filemeta(&buf) {
                            mimefactory.msg.param.set_int(Param::Width, width as i32);
                            mimefactory.msg.param.set_int(Param::Height, height as i32);
                        }
                    }
                    dc_msg_save_param_to_disk(&mut mimefactory.msg);
                }
            }
        }
        /* create message */
        if 0 == dc_mimefactory_render(&mut mimefactory) {
            dc_set_msg_failed(context, msg_id, as_opt_str(mimefactory.error));
        } else if 0
            != mimefactory
                .msg
                .param
                .get_int(Param::GuranteeE2ee)
                .unwrap_or_default()
            && 0 == mimefactory.out_encrypted
        {
            warn!(
                context,
                0,
                "e2e encryption unavailable {} - {:?}",
                msg_id,
                mimefactory.msg.param.get_int(Param::GuranteeE2ee),
            );
            dc_set_msg_failed(
                context,
                msg_id,
                Some("End-to-end-encryption unavailable unexpectedly."),
            );
        } else {
            /* unrecoverable */
            if clist_search_string_nocase(mimefactory.recipients_addr, mimefactory.from_addr)
                == 0i32
            {
                clist_insert_after(
                    mimefactory.recipients_names,
                    (*mimefactory.recipients_names).last,
                    0 as *mut libc::c_void,
                );
                clist_insert_after(
                    mimefactory.recipients_addr,
                    (*mimefactory.recipients_addr).last,
                    dc_strdup(mimefactory.from_addr) as *mut libc::c_void,
                );
            }
            if 0 != mimefactory.out_gossiped {
                chat::set_gossiped_timestamp(context, mimefactory.msg.chat_id, time());
            }
            if 0 != mimefactory.out_last_added_location_id {
                dc_set_kml_sent_timestamp(context, mimefactory.msg.chat_id, time());
                if !mimefactory.msg.hidden {
                    dc_set_msg_location_id(
                        context,
                        mimefactory.msg.id,
                        mimefactory.out_last_added_location_id,
                    );
                }
            }
            if 0 != mimefactory.out_encrypted
                && mimefactory
                    .msg
                    .param
                    .get_int(Param::GuranteeE2ee)
                    .unwrap_or_default()
                    == 0
            {
                mimefactory.msg.param.set_int(Param::GuranteeE2ee, 1);
                dc_msg_save_param_to_disk(&mut mimefactory.msg);
            }
            success = add_smtp_job(context, Action::SendMsgToSmtp, &mut mimefactory);
        }
    }

    success
}

pub fn perform_imap_jobs(context: &Context) {
    info!(context, 0, "dc_perform_imap_jobs starting.",);

    let probe_imap_network = *context.probe_imap_network.clone().read().unwrap();
    *context.probe_imap_network.write().unwrap() = false;
    *context.perform_inbox_jobs_needed.write().unwrap() = false;

    job_perform(context, Thread::Imap, probe_imap_network);
    info!(context, 0, "dc_perform_imap_jobs ended.",);
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
            info!(context, 0, "query failed: {:?}", err);
        }
    }

    for mut job in jobs.unwrap_or_default() {
        info!(
            context,
            0,
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
            &context
                .sentbox_thread
                .clone()
                .read()
                .unwrap()
                .suspend(context);
            &context
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
                Action::SendMsgToSmtp => job.do_DC_JOB_SEND(context),
                Action::DeleteMsgOnImap => job.do_DC_JOB_DELETE_MSG_ON_IMAP(context),
                Action::MarkseenMsgOnImap => job.do_DC_JOB_MARKSEEN_MSG_ON_IMAP(context),
                Action::MarkseenMdnOnImap => job.do_DC_JOB_MARKSEEN_MDN_ON_IMAP(context),
                Action::MoveMsg => job.do_DC_JOB_MOVE_MSG(context),
                Action::SetMetadata => job.do_DC_JOB_SET_METADATA(context),
                Action::GetWebPushSubscription => job.do_DC_JOB_GET_WEBPUSH_SUBSCRIPTION(context),
                Action::SendMdn => job.do_DC_JOB_SEND(context),
                Action::ConfigureImap => unsafe { dc_job_do_DC_JOB_CONFIGURE_IMAP(context, &job) },
                Action::ImexImap => unsafe { dc_job_do_DC_JOB_IMEX_IMAP(context, &job) },
                Action::MaybeSendLocations => unsafe {
                    dc_job_do_DC_JOB_MAYBE_SEND_LOCATIONS(context, &job)
                },
                Action::MaybeSendLocationsEnded => unsafe {
                    dc_job_do_DC_JOB_MAYBE_SEND_LOC_ENDED(context, &mut job)
                },
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
                0,
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
                    0,
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
                    dc_set_msg_failed(context, job.foreign_id, job.pending_error.as_ref());
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
    let mut N = 2_i32.pow((c_tries - 1) as u32);
    N = N * 60;
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

struct DeltachatMode {
    coi_enabled: bool,
    inbox_folder: String,
    configured_mvbox_folder_override: Option<String>,
}

fn determine_deltachat_mode(coi_config: &Option<CoiConfig>) -> DeltachatMode {
    // If COI is unsupported or disabled, we poll from INBOX and do not override the `mvbox_move`
    // settings. Otherwise we use "${MAILBOX_ROOT}/Chats" and "disable" `mvbox_move`, i.e.  let the
    // server do the moving of messages.
    match coi_config {
        // COI is not supported.
        | None

        // COI is supported, but not enabled.
        | Some(CoiConfig { enabled: false, .. })

        // COI is supported and enabled, but COI message filter is set to "none". Messages as
        // such will not be moved automatically from the INBOX, but DeltaChat is free to do so.
        | Some(CoiConfig {
            enabled: true,
            message_filter: CoiMessageFilter::None,
            ..
        }) => DeltachatMode {
            coi_enabled: false,
            inbox_folder: "INBOX".into(),
            configured_mvbox_folder_override: None},

        // COI is supported and enabled, message filter is set to "seen".  The server will move the
        // messages from INBOX to COI/Chats once they are marked as seen. We have to listen on
        // INBOX. XXX: We also have to change the "configured_mvbox_folder" to point to
        // "COI/Chats".
        | Some(CoiConfig {
            enabled: true,
            message_filter: CoiMessageFilter::Seen,
            mailbox_root
        }) => DeltachatMode {
            coi_enabled: true,
            inbox_folder: "INBOX".into(),
            configured_mvbox_folder_override: Some(format!("{}/Chats", mailbox_root))},

        // Active COI message filter. The server will move messages.
        Some(CoiConfig {
            enabled: true,
            message_filter: CoiMessageFilter::Active,
            mailbox_root,
        }) => DeltachatMode {
            coi_enabled: true,
            inbox_folder: format!("{}/Chats", mailbox_root),
            configured_mvbox_folder_override: Some(format!("{}/Chats", mailbox_root))},
    }
}

fn connect_to_inbox(context: &Context, inbox: &Imap) -> libc::c_int {
    let ret_connected = dc_connect_to_configured_imap(context, inbox);
    if 0 != ret_connected {
        let deltachat_mode = determine_deltachat_mode(&context.get_coi_config());

        // If `coi_enabled` is true, this will disable Deltachat from moving messages.
        context.set_coi_enabled(deltachat_mode.coi_enabled);
        inbox.set_watch_folder(deltachat_mode.inbox_folder);
        let arc = context.configured_mvbox_folder_override.clone();
        let mut mutex_guard = arc.lock().unwrap();
        *mutex_guard = deltachat_mode.configured_mvbox_folder_override;
    }
    ret_connected
}

fn send_mdn(context: &Context, msg_id: uint32_t) {
    if let Ok(mut mimefactory) = unsafe { dc_mimefactory_load_mdn(context, msg_id) } {
        if 0 != unsafe { dc_mimefactory_render(&mut mimefactory) } {
            add_smtp_job(context, Action::SendMdn, &mut mimefactory);
        }
    }
}

#[allow(non_snake_case)]
fn add_smtp_job(context: &Context, action: Action, mimefactory: &dc_mimefactory_t) -> libc::c_int {
    let pathNfilename: *mut libc::c_char;
    let mut success: libc::c_int = 0i32;
    let mut recipients: *mut libc::c_char = 0 as *mut libc::c_char;
    let mut param = Params::new();
    pathNfilename = unsafe {
        dc_get_fine_pathNfilename(
            context,
            b"$BLOBDIR\x00" as *const u8 as *const libc::c_char,
            mimefactory.rfc724_mid,
        )
    };
    if pathNfilename.is_null() {
        error!(
            context,
            0,
            "Could not find free file name for message with ID <{}>.",
            to_string(mimefactory.rfc724_mid),
        );
    } else if 0
        == unsafe {
            dc_write_file(
                context,
                pathNfilename,
                (*mimefactory.out).str_0 as *const libc::c_void,
                (*mimefactory.out).len,
            )
        }
    {
        error!(
            context,
            0,
            "Could not write message <{}> to \"{}\".",
            to_string(mimefactory.rfc724_mid),
            as_str(pathNfilename),
        );
    } else {
        recipients = unsafe {
            dc_str_from_clist(
                mimefactory.recipients_addr,
                b"\x1e\x00" as *const u8 as *const libc::c_char,
            )
        };
        param.set(Param::File, as_str(pathNfilename));
        param.set(Param::Recipients, as_str(recipients));
        job_add(
            context,
            action,
            (if mimefactory.loaded as libc::c_uint
                == DC_MF_MSG_LOADED as libc::c_int as libc::c_uint
            {
                mimefactory.msg.id
            } else {
                0
            }) as libc::c_int,
            param,
            0,
        );
        success = 1;
    }
    unsafe {
        free(recipients.cast());
        free(pathNfilename.cast());
    }
    success
}

pub fn job_add(
    context: &Context,
    action: Action,
    foreign_id: libc::c_int,
    param: Params,
    delay_seconds: i64,
) {
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
    }
}

pub fn interrupt_smtp_idle(context: &Context) {
    info!(context, 0, "Interrupting SMTP-idle...",);

    let &(ref lock, ref cvar) = &*context.smtp_state.clone();
    let mut state = lock.lock().unwrap();

    state.perform_jobs_needed = 1;
    state.idle = true;
    cvar.notify_one();
}

pub fn interrupt_imap_idle(context: &Context) {
    info!(context, 0, "Interrupting IMAP-IDLE...",);

    *context.perform_inbox_jobs_needed.write().unwrap() = true;
    context.inbox.read().unwrap().interrupt_idle();
}
