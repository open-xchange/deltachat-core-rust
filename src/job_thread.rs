use std::sync::{Arc, Condvar, Mutex};

use crate::configure::*;
use crate::context::Context;
use crate::imap::Imap;

#[derive(Debug, Copy, Clone)]
pub enum JobThreadKind {
    SentBox,
    MoveBox,
}

#[derive(Debug)]
pub struct JobThread {
    name: &'static str,
    job_thread_kind: JobThreadKind,
    pub imap: Imap,
    state: Arc<(Mutex<JobState>, Condvar)>,
}

#[derive(Clone, Debug, Default)]
pub struct JobState {
    idle: bool,
    jobs_needed: i32,
    suspended: bool,
    using_handle: bool,
}

impl JobThread {
    pub fn new(job_thread_kind: JobThreadKind, imap: Imap) -> Self {
        let name = match job_thread_kind {
            JobThreadKind::SentBox => "SENTBOX",
            JobThreadKind::MoveBox => "MVBOX",
        };
        JobThread {
            name,
            job_thread_kind,
            imap,
            state: Arc::new((Mutex::new(Default::default()), Condvar::new())),
        }
    }

    pub fn suspend(&self, context: &Context) {
        info!(context, "Suspending {}-thread.", self.name,);
        {
            self.state.0.lock().unwrap().suspended = true;
        }
        self.interrupt_idle(context);
        loop {
            let using_handle = self.state.0.lock().unwrap().using_handle;
            if !using_handle {
                return;
            }
            std::thread::sleep(std::time::Duration::from_micros(300 * 1000));
        }
    }

    pub fn unsuspend(&self, context: &Context) {
        info!(context, "Unsuspending {}-thread.", self.name);

        let &(ref lock, ref cvar) = &*self.state.clone();
        let mut state = lock.lock().unwrap();

        state.suspended = false;
        state.idle = true;
        cvar.notify_one();
    }

    pub fn interrupt_idle(&self, context: &Context) {
        {
            self.state.0.lock().unwrap().jobs_needed = 1;
        }

        info!(context, "Interrupting {}-IDLE...", self.name);

        self.imap.interrupt_idle();

        let &(ref lock, ref cvar) = &*self.state.clone();
        let mut state = lock.lock().unwrap();

        state.idle = true;
        cvar.notify_one();
    }

    pub fn fetch(&mut self, context: &Context, use_network: bool) {
        {
            let &(ref lock, _) = &*self.state.clone();
            let mut state = lock.lock().unwrap();

            if state.suspended {
                return;
            }

            state.using_handle = true;
        }

        if use_network {
            let start = std::time::Instant::now();
            if self.connect_to_imap(context) {
                info!(context, "{}-fetch started...", self.name);
                self.imap.fetch(context);

                if self.imap.should_reconnect() {
                    info!(context, "{}-fetch aborted, starting over...", self.name,);
                    self.imap.fetch(context);
                }
                info!(
                    context,
                    "{}-fetch done in {:.3} ms.",
                    self.name,
                    start.elapsed().as_millis(),
                );
            }
        }

        self.state.0.lock().unwrap().using_handle = false;
    }

    // XXX: This might be broken!
    fn folder_config_name(&self) -> &str {
        match self.job_thread_kind {
            JobThreadKind::SentBox => "configured_sentbox_folder",
            JobThreadKind::MoveBox => "configured_mvbox_folder",
        }
    }

    fn get_watch_folder(&self, context: &Context) -> Option<String> {
        if let Some(mvbox_folder_override) = context.get_mvbox_folder_override() {
            return Some(mvbox_folder_override);
        }
 
        if let Some(mvbox_name) = context.sql.get_raw_config(context, self.folder_config_name()) {
            Some(mvbox_name)
        } else {
            None
        }
    }

    fn connect_to_imap(&self, context: &Context) -> bool {
        if self.imap.is_connected() {
            return true;
        }

        let mut ret_connected = dc_connect_to_configured_imap(context, &self.imap) != 0;

        if ret_connected {
            if context
                .sql
                .get_raw_config_int(context, "folders_configured")
                .unwrap_or_default()
                < 3
            {
                self.imap.configure_folders(context, 0x1);
            }

            if let Some(mvbox_name) = self.get_watch_folder(context) {
                self.imap.set_watch_folder(mvbox_name);
            } else {
                self.imap.disconnect(context);
                ret_connected = false;
            }
        }

        ret_connected
    }

    pub fn idle(&self, context: &Context, use_network: bool) {
        {
            let &(ref lock, ref cvar) = &*self.state.clone();
            let mut state = lock.lock().unwrap();

            if 0 != state.jobs_needed {
                info!(
                    context,
                    "{}-IDLE will not be started as it was interrupted while not ideling.",
                    self.name,
                );
                state.jobs_needed = 0;
                return;
            }

            if state.suspended {
                while !state.idle {
                    state = cvar.wait(state).unwrap();
                }
                state.idle = false;
                return;
            }

            state.using_handle = true;

            if !use_network {
                state.using_handle = false;

                while !state.idle {
                    state = cvar.wait(state).unwrap();
                }
                state.idle = false;
                return;
            }
        }

        self.connect_to_imap(context);
        info!(context, "{}-IDLE started...", self.name,);
        self.imap.idle(context);
        info!(context, "{}-IDLE ended.", self.name);

        self.state.0.lock().unwrap().using_handle = false;
    }
}
