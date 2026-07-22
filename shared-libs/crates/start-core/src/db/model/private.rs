use std::collections::BTreeMap;

use patch_db::{HasModel, Value};
use serde::{Deserialize, Serialize};

use crate::PackageId;
use crate::auth::AuthKeys;
use crate::backup::target::cifs::CifsTargets;
use crate::net::forward::AvailablePorts;
use crate::net::keys::KeyStore;
use crate::notifications::Notifications;
use crate::prelude::*;
use crate::ssh::SshKeys;
use crate::util::serde::Pem;

#[derive(Debug, Deserialize, Serialize, HasModel)]
#[serde(rename_all = "camelCase")]
#[model = "Model<Self>"]
pub struct Private {
    pub key_store: KeyStore,
    pub password: String, // argon2 hash
    #[serde(default)]
    pub session_pubkeys: AuthKeys,
    #[serde(default = "generate_developer_key")]
    pub developer_key: Pem<ed25519_dalek::SigningKey>,
    pub ssh_privkey: Pem<ssh_key::PrivateKey>,
    pub ssh_pubkeys: SshKeys,
    pub available_ports: AvailablePorts,
    pub notifications: Notifications,
    pub cifs: CifsTargets,
    #[serde(default)]
    pub package_stores: BTreeMap<PackageId, Value>,
}

pub fn generate_developer_key() -> Pem<ed25519_dalek::SigningKey> {
    Pem(ed25519_dalek::SigningKey::generate(
        &mut crate::util::crypto::os_rng(),
    ))
}
