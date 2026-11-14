//! Password-only data encryption with TPM-gated bootstrap credentials.
//!
//! The admin password independently unlocks the data volumes. Boot attestation
//! gates the TLS identity and network credentials used to request that password,
//! never the data key. See `projects/start-os/FDE.md` for the boot contract.

pub mod api;
pub mod boot;
pub mod bootstrap;
pub mod credentials;
pub mod enrollment;
pub mod initramfs;
pub mod loader;
pub mod memory;
pub mod network;
mod root;
pub mod source;
pub mod tpm;
pub mod unlock;
