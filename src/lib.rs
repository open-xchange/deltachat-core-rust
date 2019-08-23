#![deny(clippy::correctness)]
// TODO: make all of these errors, such that clippy actually passes.
#![warn(clippy::all, clippy::perf, clippy::not_unsafe_ptr_arg_deref)]
// This is nice, but for now just annoying.
#![allow(clippy::unreadable_literal)]
#![feature(ptr_wrapping_offset_from)]

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
mod log;
#[macro_use]
pub mod error;

mod aheader;
pub mod chat;
pub mod chatlist;
pub mod coi;
pub mod config;
pub mod configure;
pub mod constants;
pub mod contact;
pub mod context;
mod imap;
pub mod job;
mod job_thread;
pub mod key;
pub mod keyring;
pub mod lot;
pub mod message;
pub mod oauth2;
mod param;
pub mod peerstate;
pub mod pgp;
pub mod qr;
mod smtp;
pub mod sql;
mod stock;
pub mod types;
pub mod webpush;
pub mod x;

mod coi_feature;
mod coi_message_filter;
pub mod dc_array;
mod dc_dehtml;
mod dc_e2ee;
pub mod dc_imex;
pub mod dc_location;
mod dc_loginparam;
mod dc_mimefactory;
pub mod dc_mimeparser;
mod dc_move;
pub mod dc_receive_imf;
pub mod dc_securejoin;
mod dc_simplify;
mod dc_strencode;
mod dc_token;
pub mod dc_tools;

#[cfg(test)]
mod test_utils;
