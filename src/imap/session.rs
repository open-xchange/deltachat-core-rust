use async_imap::{
    error::Result as ImapResult,
    extensions::metadata::MetadataDepth,
    types::{Capabilities, Fetch, Mailbox, Name},
    Session as ImapSession,
};
use imap_proto::types::Metadata;
use async_native_tls::TlsStream;
use async_std::net::TcpStream;
use async_std::prelude::*;

#[derive(Debug)]
pub(crate) enum Session {
    Secure(ImapSession<TlsStream<TcpStream>>),
    Insecure(ImapSession<TcpStream>),
}

impl Session {
    pub async fn capabilities(&mut self) -> ImapResult<Capabilities> {
        let res = match self {
            Session::Secure(i) => i.capabilities().await?,
            Session::Insecure(i) => i.capabilities().await?,
        };

        Ok(res)
    }

    pub async fn list(
        &mut self,
        reference_name: Option<&str>,
        mailbox_pattern: Option<&str>,
    ) -> ImapResult<Vec<Name>> {
        let res = match self {
            Session::Secure(i) => {
                i.list(reference_name, mailbox_pattern)
                    .await?
                    .collect::<ImapResult<_>>()
                    .await?
            }
            Session::Insecure(i) => {
                i.list(reference_name, mailbox_pattern)
                    .await?
                    .collect::<ImapResult<_>>()
                    .await?
            }
        };
        Ok(res)
    }

    pub async fn create<S: AsRef<str>>(&mut self, mailbox_name: S) -> ImapResult<()> {
        match self {
            Session::Secure(i) => i.create(mailbox_name).await?,
            Session::Insecure(i) => i.create(mailbox_name).await?,
        }
        Ok(())
    }

    pub async fn subscribe<S: AsRef<str>>(&mut self, mailbox: S) -> ImapResult<()> {
        match self {
            Session::Secure(i) => i.subscribe(mailbox).await?,
            Session::Insecure(i) => i.subscribe(mailbox).await?,
        }
        Ok(())
    }

    pub async fn close(&mut self) -> ImapResult<()> {
        match self {
            Session::Secure(i) => i.close().await?,
            Session::Insecure(i) => i.close().await?,
        }
        Ok(())
    }

    pub async fn select<S: AsRef<str>>(&mut self, mailbox_name: S) -> ImapResult<Mailbox> {
        let mbox = match self {
            Session::Secure(i) => i.select(mailbox_name).await?,
            Session::Insecure(i) => i.select(mailbox_name).await?,
        };

        Ok(mbox)
    }

    pub async fn fetch<S1, S2>(&mut self, sequence_set: S1, query: S2) -> ImapResult<Vec<Fetch>>
    where
        S1: AsRef<str>,
        S2: AsRef<str>,
    {
        let res = match self {
            Session::Secure(i) => {
                i.fetch(sequence_set, query)
                    .await?
                    .collect::<ImapResult<_>>()
                    .await?
            }
            Session::Insecure(i) => {
                i.fetch(sequence_set, query)
                    .await?
                    .collect::<ImapResult<_>>()
                    .await?
            }
        };
        Ok(res)
    }

    pub async fn uid_fetch<S1, S2>(&mut self, uid_set: S1, query: S2) -> ImapResult<Vec<Fetch>>
    where
        S1: AsRef<str>,
        S2: AsRef<str>,
    {
        let res = match self {
            Session::Secure(i) => {
                i.uid_fetch(uid_set, query)
                    .await?
                    .collect::<ImapResult<_>>()
                    .await?
            }
            Session::Insecure(i) => {
                i.uid_fetch(uid_set, query)
                    .await?
                    .collect::<ImapResult<_>>()
                    .await?
            }
        };

        Ok(res)
    }

    pub async fn uid_store<S1, S2>(&mut self, uid_set: S1, query: S2) -> ImapResult<Vec<Fetch>>
    where
        S1: AsRef<str>,
        S2: AsRef<str>,
    {
        let res = match self {
            Session::Secure(i) => {
                i.uid_store(uid_set, query)
                    .await?
                    .collect::<ImapResult<_>>()
                    .await?
            }
            Session::Insecure(i) => {
                i.uid_store(uid_set, query)
                    .await?
                    .collect::<ImapResult<_>>()
                    .await?
            }
        };
        Ok(res)
    }

    pub async fn uid_mv<S1: AsRef<str>, S2: AsRef<str>>(
        &mut self,
        uid_set: S1,
        mailbox_name: S2,
    ) -> ImapResult<()> {
        match self {
            Session::Secure(i) => i.uid_mv(uid_set, mailbox_name).await?,
            Session::Insecure(i) => i.uid_mv(uid_set, mailbox_name).await?,
        }
        Ok(())
    }

    pub async fn uid_copy<S1: AsRef<str>, S2: AsRef<str>>(
        &mut self,
        uid_set: S1,
        mailbox_name: S2,
    ) -> ImapResult<()> {
        match self {
            Session::Secure(i) => i.uid_copy(uid_set, mailbox_name).await?,
            Session::Insecure(i) => i.uid_copy(uid_set, mailbox_name).await?,
        }

        Ok(())
    }

    pub async fn get_metadata<S: AsRef<str>>(
        &mut self,
        mbox: S,
        key: &[S],
        depth: MetadataDepth,
        max_size: Option<usize>,
    ) -> ImapResult<Vec<Metadata>> {
        let res = match self {
            Session::Secure(i) => i.get_metadata(mbox, key, depth, max_size).await?,
            Session::Insecure(i) => i.get_metadata(mbox, key, depth, max_size).await?,
        };
        Ok(res)
    }

    pub async fn set_metadata<S: AsRef<str>>(
        &mut self,
        mbox: S,
        keyval: &[Metadata],
    ) -> ImapResult<()> {
        match self {
            Session::Secure(i) => i.set_metadata(mbox, keyval).await?,
            Session::Insecure(i) => i.set_metadata(mbox, keyval).await?,
        }
        Ok(())
    }
}
