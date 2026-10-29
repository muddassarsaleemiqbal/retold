//! FilmCraft's JSON-line protocol. Connections are to a caller-owned loopback instance;
//! this crate never discovers or attaches to the user's existing editor session.

#![deny(clippy::print_stdout, clippy::print_stderr)]

pub mod client;
pub mod command;

pub use client::{Client, Error};
pub use command::Command;

pub mod timeline;

pub mod editor;
pub mod session;

pub mod ducking;
