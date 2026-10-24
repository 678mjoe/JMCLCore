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

#[cfg(target_os = "linux")]
mod numbers;
