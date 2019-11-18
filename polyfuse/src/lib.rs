#![doc(html_root_url = "https://docs.rs/polyfuse/0.1.1")]

//! A FUSE (Filesystem in userspace) framework.

#![warn(clippy::checked_conversions)]
#![deny(
    missing_docs,
    missing_debug_implementations,
    clippy::cast_lossless,
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    clippy::cast_precision_loss,
    clippy::cast_sign_loss,
    clippy::invalid_upcast_comparisons
)]
#![forbid(clippy::unimplemented)]

pub mod notify;
pub mod reply;
pub mod request;

mod common;
mod dirent;
mod fs;
mod init;
mod session;

#[doc(inline)]
pub use crate::{
    common::{FileAttr, FileLock, Forget, StatFs},
    dirent::DirEntry,
    fs::{Context, Filesystem, Operation},
    init::{CapabilityFlags, ConnectionInfo, SessionInitializer},
    notify::Notifier,
    request::Buffer,
    session::{Interrupt, Session},
};