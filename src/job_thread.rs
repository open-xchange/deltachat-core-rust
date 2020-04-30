use std::sync::{Arc, Condvar, Mutex};

use crate::context::Context;
use crate::error::{format_err, Result};
use crate::imap::Imap;

#[derive(Clone, Debug)]
pub enum FolderSpec {
    InboxFolder,
    SentboxFolder,
    MvboxFolder,
}

impl FolderSpec {
    pub fn to_name(&self) -> &'static str {
        match self {
            Self::InboxFolder => "INBOX",
            Self::SentboxFolder => "SENTBOX",
            Self::MvboxFolder => "MVBOX",
        }
    }
}

#[derive(Debug)]
pub struct JobThread {
    folder_spec: FolderSpec,
    watch_folder: Option<String>,
    pub imap: Imap,
    state: Arc<(Mutex<JobState>, Condvar)>,
}

#[derive(Clone, Debug, Default)]
pub struct JobState {
    idle: bool,
    jobs_needed: bool,
    suspended: bool,
    using_handle: bool,
}

impl JobThread {
    pub fn new(folder_spec: FolderSpec, imap: Imap) -> Self {
        JobThread {
            folder_spec,
            watch_folder: None,
            imap,
            state: Arc::new((Mutex::new(Default::default()), Condvar::new())),
        }
    }

    pub fn suspend(&self, context: &Context) {
        info!(context, "Suspending {}-thread.", self.folder_spec.to_name(),);
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
        info!(context, "Unsuspending {}-thread.", self.folder_spec.to_name());

        let &(ref lock, ref cvar) = &*self.state.clone();
        let mut state = lock.lock().unwrap();

        state.suspended = false;
        state.idle = true;
        cvar.notify_one();
    }

    pub fn interrupt_idle(&self, context: &Context) {
        {
            self.state.0.lock().unwrap().jobs_needed = true;
        }

        info!(context, "Interrupting {}-IDLE...", self.folder_spec.to_name());

        self.imap.interrupt_idle(context);

        let &(ref lock, ref cvar) = &*self.state.clone();
        let mut state = lock.lock().unwrap();

        state.idle = true;
        cvar.notify_one();
        info!(context, "Interrupting {}-IDLE... finished", self.folder_spec.to_name());
    }

    pub async fn fetch(&mut self, context: &Context, use_network: bool) {
        {
            let &(ref lock, _) = &*self.state.clone();
            let mut state = lock.lock().unwrap();

            if state.suspended {
                return;
            }

            state.using_handle = true;
        }

        if use_network {
            if let Err(err) = self.connect_and_fetch(context).await {
                warn!(context, "connect+fetch failed: {}, reconnect & retry", err);
                self.imap.trigger_reconnect(context);
                if let Err(err) = self.connect_and_fetch(context).await {
                    warn!(context, "connect+fetch failed: {}", err);
                }
            }
        }
        self.state.0.lock().unwrap().using_handle = false;
    }

    async fn connect_and_fetch(&mut self, context: &Context) -> Result<()> {
        let prefix = format!("{}-fetch", self.folder_spec.to_name());
        self.imap.connect_configured(context)?;
        self.watch_folder = Self::folder_name(context, self.folder_spec.clone());
        if let Some(watch_folder) = self.watch_folder.clone() {
            let start = std::time::Instant::now();
            info!(context, "{} started...", prefix);
            let res = self
                .imap
                .fetch(context, watch_folder.as_str())
                .await
                .map_err(Into::into);
            let elapsed = start.elapsed().as_millis();
            info!(context, "{} done in {:.3} ms.", prefix, elapsed);

            res
        } else {
            Err(format_err!("WatchFolder not found: not-set"))
        }
    }

    fn folder_name(context:&Context, folder_spec: FolderSpec) -> Option<String> {
        let folders = context.config_folders.read().unwrap().clone();
        if folders.is_none() {
            return None;
        }
        match folder_spec {
            FolderSpec::InboxFolder => Some("INBOX".to_string()),
            FolderSpec::SentboxFolder => Some(folders.unwrap().sentbox_folder),
            FolderSpec::MvboxFolder => Some(folders.unwrap().movebox_folder),
        }
    }

    pub fn idle(&self, context: &Context, use_network: bool) {
        {
            let &(ref lock, ref cvar) = &*self.state.clone();
            let mut state = lock.lock().unwrap();

            if state.jobs_needed {
                info!(
                    context,
                    "{}-IDLE will not be started as it was interrupted while not idling.",
                    self.folder_spec.to_name(),
                );
                state.jobs_needed = false;
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

        let prefix = format!("{}-IDLE", self.folder_spec.to_name());
        let do_fake_idle = match self.imap.connect_configured(context) {
            Ok(()) => {
                if !self.imap.can_idle() {
                    true // we have to do fake_idle
                } else {
                    if let Some(watch_folder) = self.watch_folder.clone() {
                        info!(context, "{} started...", prefix);
                        let res = self.imap.idle(context, watch_folder);
                        info!(context, "{} ended...", prefix);
                        if let Err(err) = res {
                            warn!(context, "{} failed: {} -> reconnecting", prefix, err);
                            // something is Label { Label }orked, let's start afresh on the next occassion
                            self.imap.disconnect(context);
                        }
                    }
                    false
                }
            }
            Err(err) => {
                info!(context, "{}-IDLE connection fail: {:?}", self.folder_spec.to_name(), err);
                // if the connection fails, use fake_idle to retry periodically
                // fake_idle() will be woken up by interrupt_idle() as
                // well so will act on maybe_network events
                true
            }
        };
        if do_fake_idle {
            let watch_folder = self.watch_folder.clone();
            self.imap.fake_idle(context, watch_folder);
        }

        self.state.0.lock().unwrap().using_handle = false;
    }
}
