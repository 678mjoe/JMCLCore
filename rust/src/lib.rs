//! Experimental native Rust implementation. Only explicitly inventoried behavior is compatible.
pub mod protocol;
pub const NAME: &str = "jmcl-core";
pub const VERSION: &str = "0.1.0-dev";
pub const PROTOCOL: u32 = 1;
pub const ORACLE_COMMIT: &str = "5a4310debbcee26b3d2391bab2339f79cd6ec52e";

pub mod store;

pub mod accounts;
pub mod instances;
pub mod servers;

pub mod content;
pub mod java;
pub mod metadata;
pub mod plan;
pub mod version;
pub mod worlds;

pub mod auth;
pub mod install;

pub mod launch;
pub mod modpack;
