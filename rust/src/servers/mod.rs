//! Portable server codecs and Linux lifecycle/world-set adapters.
//! Server installation and remote console/query integrations are separate slices.
#[cfg(target_os = "linux")]
pub mod lifecycle;
pub mod manifest;
pub mod properties;
pub mod rpc;
#[cfg(target_os = "linux")]
pub mod store;
pub use rpc::handle;
mod cli;
pub use cli::cli;

pub mod creation;
mod creation_cli;
#[cfg(target_os = "linux")]
mod numbers;
#[cfg(target_os = "linux")]
pub mod planning;
pub mod query;
pub mod rcon;
pub mod remote;
mod remote_cli;

pub mod installation;

mod installation_cli;

#[cfg(target_os = "linux")]
mod params;
