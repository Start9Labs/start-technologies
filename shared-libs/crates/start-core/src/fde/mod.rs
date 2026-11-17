//! Password-only data encryption with TPM-gated bootstrap credentials.
//!
//! The admin password independently unlocks the data volumes. Boot attestation
//! gates the TLS identity and network credentials used to request that password,
//! never the data key. See `projects/start-os/FDE.md` for the boot contract.

pub mod api;
pub mod boot;
pub(crate) mod boot_services;
pub mod bootstrap;
#[cfg(target_os = "linux")]
pub(crate) mod capture;
pub mod credentials;
pub(crate) mod enable;
pub mod enrollment;
pub(crate) mod generation;
pub mod initramfs;
pub mod loader;
pub mod memory;
mod namespace;
pub mod network;
pub(crate) mod private_config;
mod root;
pub mod source;
pub mod tpm;
pub mod unlock;
