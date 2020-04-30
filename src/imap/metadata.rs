use super::Imap;
use async_std::task;
use std::str::FromStr;

use crate::coi::{ CoiMessageFilter, CoiConfig };
use crate::context::Context;
use crate::webpush::WebPushConfig;
// use async_imap::{get_metadata, set_metadata};
pub use async_imap::extensions::metadata::MetadataDepth;
pub use imap_proto::types::Metadata;

impl Imap {
    pub(crate) fn get_metadata<S: AsRef<str>>(
        &self,
        context: &Context,
        mbox: S,
        key: &[S],
        depth: MetadataDepth,
        max_size: Option<usize>,
    ) -> super::Result<Vec<Metadata>>
    {
        info!(context, "get metadata");
        task::block_on(async{
            if let Some(ref mut session) = &mut *self.session.lock().await {
                match session.get_metadata(mbox, key, depth, max_size).await {
                    Ok(res) => Ok(res),
                    Err(_) => Err(super::Error::Other(String::from("GETMETADATA failed"))),
                }
            } else {
                Err(super::Error::NoConnection)
            }})
    }
    
    pub(crate) fn set_metadata<S: AsRef<str>>(
        &self,
        context: &Context,
        mbox: S,
        keyval: &[Metadata],
    ) -> super::Result<()>
    where
        S: std::fmt::Debug,
    {
        info!(context, "set metadata");

        task::block_on(async{
        if let Some(ref mut session) = &mut *self.session.lock().await {
            // let res = session.set_metadata(mbox, keyval).await?;
            match session.set_metadata(mbox, keyval).await {
                Ok(res) => Ok(res),
                Err(_) => Err(super::Error::Other(String::from("SETMETADATA failed"))),
            }
        } else {
            Err(super::Error::NoConnection)
        }})
    }

    pub(crate) async fn update_metadata(
        &self,
        context: &Context,
        has_coi: Option<bool>,
        has_webpush: Option<bool>,
    ) {
        let (coi, webpush) = self.query_metadata(
            context,
            has_coi.unwrap_or(self.config.read().await.coi.is_some()),
            has_webpush.unwrap_or(self.config.read().await.webpush.is_some()),
        );
        let mut config = self.config.write().await;
        config.coi = coi;
        config.webpush = webpush;
    }
    
    fn query_metadata (
        &self,
        context: &Context,
        has_coi: bool,
        has_webpush: bool,
    ) -> (Option<CoiConfig>, Option<WebPushConfig>) {
        if !has_coi && !has_webpush {
            return (None, None);
        }

        let mut keys = vec![];
        let mut coi = None;
        let mut webpush = None;
        if has_coi {
            keys.push("/private/vendor/vendor.dovecot/coi/config");
            coi = Some(CoiConfig::default());
        }
        if has_webpush {
            keys.push("/private/vendor/vendor.dovecot/webpush");
            webpush = Some(WebPushConfig::default());
        }

        let metadata = self.get_metadata(context, "", &keys, MetadataDepth::One, None);

        if let Ok(metadata) = metadata {
            for meta in metadata {
                match meta.entry.as_str() {
                    "/private/vendor/vendor.dovecot/coi/config/mailbox-root" => {
                        if coi.is_some() && meta.value.is_some() {
                            coi.as_mut()
                                .unwrap()
                                .coi_root = meta.value.unwrap();
                        }
                    }
                    "/private/vendor/vendor.dovecot/coi/config/enabled" => {
                        if let Some(ref mut c) = coi {
                            if let Some(val) = meta.value {
                                c.enabled = val == "yes";
                            }
                        }
                    }
                    "/private/vendor/vendor.dovecot/coi/config/message-filter" => {
                        if meta.value.is_some() {
                            if let Ok(message_filter) = CoiMessageFilter::from_str(meta.value.unwrap().as_str()) {
                                if let Some(ref mut c) = coi {
                                    c.message_filter = message_filter;
                                }
                            }
                        }
                    }
                    "/private/vendor/vendor.dovecot/webpush/vapid" => {
                        if webpush.is_some() {
                            webpush.as_mut().unwrap().vapid = meta.value.map(|s| s.to_string());
                        }
                    }
                    _ => {
                        if meta.value.is_some() {
                            info!(
                                context,
                                "Unknown metadata: {} = {}", meta.entry, meta.value.unwrap()
                            );
                        }
                    }
                }
            }
        } else if let Err(error) = metadata {
            warn!(context, "Error while retrieving metadata: {}", error);
        }
        (coi, webpush)
    }
}
