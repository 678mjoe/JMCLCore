//! Modpack compatibility layer. Inspection is internal: the public protocol only exposes install.
pub mod schema;
pub use schema::{Error, Loader, Selection};
pub mod archive;
pub mod planning;

pub mod provider;

pub mod service;

pub mod rpc;

pub mod cli;
