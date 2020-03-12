#![forbid(unsafe_code)]
#![deny(clippy::correctness, missing_debug_implementations, clippy::all)]
#![allow(clippy::match_bool)]

#[macro_use]
extern crate failure_derive;
#[macro_use]
extern crate num_derive;
#[macro_use]
extern crate smallvec;
#[macro_use]
extern crate rusqlite;
extern crate strum;
#[macro_use]
extern crate strum_macros;
#[macro_use]
extern crate debug_stub_derive;

#[macro_use]
pub mod log;
#[macro_use]
pub mod error;

pub mod headerdef;

pub(crate) mod events;
pub use events::*;

mod aheader;
pub mod blob;
pub mod chat;
pub mod chatlist;
pub mod coi;
pub mod config;
pub mod configure;
pub mod constants;
pub mod contact;
pub mod context;
pub mod e2ee;
mod imap;
mod imap_client;
pub mod imex;
#[macro_use]
pub mod job;
mod job_thread;
pub mod key;
pub mod keyring;
pub mod location;
mod login_param;
pub mod lot;
pub mod message;
mod mimefactory;
pub mod mimeparser;
pub mod oauth2;
mod param;
pub mod peerstate;
pub mod pgp;
pub mod provider;
pub mod qr;
pub mod securejoin;
mod simplify;
mod smtp;
pub mod sql;
pub mod webpush;
pub mod stock;
mod token;
#[macro_use]
mod dehtml;

pub mod dc_receive_imf;
pub mod dc_tools;

/// if set imap/incoming and smtp/outgoing MIME messages will be printed
pub const DCC_MIME_DEBUG: &str = "DCC_MIME_DEBUG";

/// if set IMAP protocol commands and responses will be printed
pub const DCC_IMAP_DEBUG: &str = "DCC_IMAP_DEBUG";

#[cfg(test)]
mod test_utils;
